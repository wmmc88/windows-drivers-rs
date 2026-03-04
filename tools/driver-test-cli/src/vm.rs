//! Hyper-V VM lifecycle management via PowerShell.

use crate::ps::{run_ps_json_retry, sanitize_ps_string, validate_vm_name, PsError};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info};

const DEFAULT_MAX_RETRIES: usize = 3;
const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Serialize)]
pub struct TestVm {
    pub name: String,
    pub state: String,
    pub memory_mb: u32,
    pub cpus: u8,
    pub generation: u8,
}

#[derive(Debug, Clone)]
pub struct SnapshotId(pub String);

#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Error)]
pub enum VmError {
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("PowerShell error: {0}")]
    Ps(#[from] PsError),
    #[error("invalid VM name: {0}")]
    InvalidName(String),
    #[error("timeout waiting for VM operation")]
    Timeout,
}

/// Abstraction for VM operations, enabling mock implementations for testing.
pub trait VmProvider {
    fn create_vm(&self, name: &str, memory_mb: u32, cpus: u8) -> Result<TestVm, VmError>;
    fn get_vm(&self, name: &str) -> Result<Option<TestVm>, VmError>;
    fn ensure_running(&self, vm: &TestVm) -> Result<TestVm, VmError>;
    fn snapshot(&self, vm: &TestVm, label: &str) -> Result<SnapshotId, VmError>;
    fn revert_snapshot(&self, vm: &TestVm, snap: &SnapshotId) -> Result<TestVm, VmError>;
    fn execute(&self, vm: &TestVm, command: &str) -> Result<CommandOutput, VmError>;
    fn copy_file(&self, vm: &TestVm, src: &Path, guest_dest: &str) -> Result<(), VmError>;
    fn remove_vm(&self, vm: &TestVm) -> Result<(), VmError>;
}

/// Production Hyper-V provider using PowerShell cmdlets.
pub struct HypervProvider {
    pub max_retries: usize,
    pub retry_delay: Duration,
}

impl Default for HypervProvider {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            retry_delay: DEFAULT_RETRY_DELAY,
        }
    }
}

impl HypervProvider {
    fn ps(&self, script: &str) -> Result<serde_json::Value, VmError> {
        run_ps_json_retry(script, self.max_retries, self.retry_delay).map_err(VmError::from)
    }
}

fn parse_vm(v: &serde_json::Value) -> Option<TestVm> {
    let name = v.get("Name")?.as_str()?.to_string();
    // State can be numeric (2 = Running) or string
    let state = match v.get("State") {
        Some(serde_json::Value::Number(n)) => match n.as_u64() {
            Some(2) => "Running",
            Some(3) => "Off",
            Some(6) => "Saved",
            Some(9) => "Paused",
            _ => "Unknown",
        }
        .to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => "Unknown".to_string(),
    };
    let mem_bytes = v.get("MemoryStartup").and_then(|m| m.as_u64()).unwrap_or(0);
    let memory_mb = (mem_bytes / (1024 * 1024)) as u32;
    let cpus = v
        .get("ProcessorCount")
        .and_then(|c| c.as_u64())
        .unwrap_or(1) as u8;
    let generation = v
        .get("Generation")
        .and_then(|g| g.as_u64())
        .unwrap_or(2) as u8;
    Some(TestVm { name, state, memory_mb, cpus, generation })
}

fn is_vm_missing(msg: &str) -> bool {
    let s = msg.to_ascii_lowercase();
    s.contains("unable to find a virtual machine")
        || s.contains("cannot find a virtual machine")
        || s.contains("was not found")
}

impl VmProvider for HypervProvider {
    fn create_vm(&self, name: &str, memory_mb: u32, cpus: u8) -> Result<TestVm, VmError> {
        validate_vm_name(name).map_err(VmError::InvalidName)?;
        let safe_name = sanitize_ps_string(name);
        let bytes = memory_mb as u64 * 1024 * 1024;
        info!(%name, memory_mb, cpus, "creating VM");
        let script = format!(
            "New-VM -Name '{safe_name}' -MemoryStartupBytes {bytes} -Generation 2; \
             Set-VMProcessor -VMName '{safe_name}' -Count {cpus}; \
             Get-VM -Name '{safe_name}' | Select-Object Name,State,MemoryStartup,ProcessorCount,Generation | ConvertTo-Json -Compress"
        );
        let val = self.ps(&script)?;
        parse_vm(&val).ok_or_else(|| VmError::Ps(PsError::Json {
            stdout: val.to_string(),
            error: "unexpected VM object shape".into(),
        }))
    }

    fn get_vm(&self, name: &str) -> Result<Option<TestVm>, VmError> {
        validate_vm_name(name).map_err(VmError::InvalidName)?;
        let safe_name = sanitize_ps_string(name);
        let script = format!(
            "Get-VM -Name '{safe_name}' | Select-Object Name,State,MemoryStartup,ProcessorCount,Generation | ConvertTo-Json -Compress"
        );
        match self.ps(&script) {
            Ok(v) => {
                if v.is_array() {
                    Ok(v.as_array().and_then(|a| a.first()).and_then(parse_vm))
                } else {
                    Ok(parse_vm(&v))
                }
            }
            Err(VmError::Ps(PsError::Fail { ref stderr, .. })) if is_vm_missing(stderr) => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn ensure_running(&self, vm: &TestVm) -> Result<TestVm, VmError> {
        if vm.state.eq_ignore_ascii_case("running") {
            return Ok(vm.clone());
        }
        let safe = sanitize_ps_string(&vm.name);
        info!(vm = %vm.name, "starting VM");
        let script = format!(
            "Start-VM -Name '{safe}' -WarningAction SilentlyContinue; \
             Start-Sleep -Seconds 5; \
             Get-VM -Name '{safe}' | Select-Object Name,State,MemoryStartup,ProcessorCount,Generation | ConvertTo-Json -Compress"
        );
        let val = self.ps(&script)?;
        parse_vm(&val).ok_or(VmError::Timeout)
    }

    fn snapshot(&self, vm: &TestVm, label: &str) -> Result<SnapshotId, VmError> {
        let safe = sanitize_ps_string(&vm.name);
        let safe_label = sanitize_ps_string(label);
        info!(vm = %vm.name, %label, "creating snapshot");
        let script = format!(
            "Checkpoint-VM -Name '{safe}' -SnapshotName '{safe_label}' -Confirm:$false; \
             Write-Output ('{{\"name\":\"{safe_label}\"}}') | ConvertTo-Json -Compress"
        );
        let _ = self.ps(&script)?;
        Ok(SnapshotId(label.to_string()))
    }

    fn revert_snapshot(&self, vm: &TestVm, snap: &SnapshotId) -> Result<TestVm, VmError> {
        let safe = sanitize_ps_string(&vm.name);
        let safe_snap = sanitize_ps_string(&snap.0);
        info!(vm = %vm.name, snapshot = %snap.0, "reverting snapshot");
        let script = format!(
            "$snap = Get-VMSnapshot -VMName '{safe}' -Name '{safe_snap}' | Sort-Object CreationTime -Descending | Select-Object -First 1; \
             Restore-VMSnapshot -VMSnapshot $snap -Confirm:$false; \
             Start-VM -Name '{safe}' -ErrorAction SilentlyContinue; \
             Start-Sleep -Seconds 5; \
             Get-VM -Name '{safe}' | Select-Object Name,State,MemoryStartup,ProcessorCount,Generation | ConvertTo-Json -Compress"
        );
        let val = self.ps(&script)?;
        parse_vm(&val).ok_or(VmError::Timeout)
    }

    fn execute(&self, vm: &TestVm, command: &str) -> Result<CommandOutput, VmError> {
        let safe = sanitize_ps_string(&vm.name);
        // Use a ScriptBlock that captures output and exit code
        debug!(vm = %vm.name, cmd_len = command.len(), "executing in guest");
        let script = format!(
            "$result = Invoke-Command -VMName '{safe}' -ScriptBlock {{ {command} }} 2>&1 | Out-String; \
             Write-Output $result"
        );
        let output = crate::ps::run_ps(&script)?;
        Ok(CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
        })
    }

    fn copy_file(&self, vm: &TestVm, src: &Path, guest_dest: &str) -> Result<(), VmError> {
        let safe = sanitize_ps_string(&vm.name);
        let src_str = src.display().to_string();
        let safe_src = sanitize_ps_string(&src_str);
        let safe_dest = sanitize_ps_string(guest_dest);
        info!(vm = %vm.name, src = %src_str, dest = %guest_dest, "copying file to guest");
        let script = format!(
            "Copy-VMFile -Name '{safe}' -SourcePath '{safe_src}' -DestinationPath '{safe_dest}' -FileSource Host -CreateFullPath -Force; \
             Write-Output '{{\"ok\":true}}'"
        );
        let _ = self.ps(&script)?;
        Ok(())
    }

    fn remove_vm(&self, vm: &TestVm) -> Result<(), VmError> {
        let safe = sanitize_ps_string(&vm.name);
        info!(vm = %vm.name, "removing VM");
        let script = format!(
            "Stop-VM -Name '{safe}' -Force -ErrorAction SilentlyContinue; \
             Remove-VM -Name '{safe}' -Force; \
             Write-Output '{{\"removed\":true}}'"
        );
        let _ = self.ps(&script)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vm_from_json() {
        let json: serde_json::Value = serde_json::json!({
            "Name": "test-vm",
            "State": 2,
            "MemoryStartup": 2147483648_u64,
            "ProcessorCount": 4,
            "Generation": 2
        });
        let vm = parse_vm(&json).unwrap();
        assert_eq!(vm.name, "test-vm");
        assert_eq!(vm.state, "Running");
        assert_eq!(vm.memory_mb, 2048);
        assert_eq!(vm.cpus, 4);
        assert_eq!(vm.generation, 2);
    }

    #[test]
    fn parse_vm_string_state() {
        let json: serde_json::Value = serde_json::json!({
            "Name": "my-vm",
            "State": "Off",
            "MemoryStartup": 1073741824_u64,
            "ProcessorCount": 2,
            "Generation": 1
        });
        let vm = parse_vm(&json).unwrap();
        assert_eq!(vm.state, "Off");
        assert_eq!(vm.memory_mb, 1024);
    }

    #[test]
    fn vm_missing_detection() {
        assert!(is_vm_missing("Hyper-V was unable to find a virtual machine with name"));
        assert!(is_vm_missing("Cannot find a virtual machine with name"));
        assert!(!is_vm_missing("Access is denied"));
    }
}
