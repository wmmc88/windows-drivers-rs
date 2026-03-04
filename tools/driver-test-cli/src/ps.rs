//! PowerShell execution wrapper.
//!
//! All Hyper-V and guest interactions funnel through [`run_ps`] which wraps
//! commands in structured error handling and returns parsed JSON output.

use std::process::Command;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum PsError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("PowerShell failed (exit {exit_code}): {stderr}")]
    Fail {
        command: String,
        stdout: String,
        stderr: String,
        exit_code: i32,
    },

    #[error("JSON parse error: {error}\nstdout: {stdout}")]
    Json {
        stdout: String,
        error: String,
    },

    #[error("timeout after {0:?}")]
    Timeout(Duration),
}

impl PsError {
    /// Whether this error is transient and eligible for retry.
    pub fn is_transient(&self) -> bool {
        match self {
            PsError::Fail { stderr, .. } => classify_transient(stderr),
            PsError::Timeout(_) => true,
            _ => false,
        }
    }
}

/// Patterns indicating the error is transient and retry-eligible.
fn classify_transient(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("the virtual machine is not ready")
        || s.contains("a remote session might have ended")
        || s.contains("failed to establish a connection")
        || s.contains("an error has occurred which powershell cannot handle")
}

/// Result of a PowerShell execution.
#[derive(Debug)]
pub struct PsOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute a PowerShell script and return raw output.
///
/// Uses `-NoLogo -NoProfile -ExecutionPolicy Bypass` for minimal startup.
/// The script is wrapped in try/catch that serializes errors to stderr as JSON.
pub fn run_ps(script: &str) -> Result<PsOutput, PsError> {
    let wrapped = format!(
        "$ErrorActionPreference='Stop'; try {{ {} }} catch {{ $_ | ConvertTo-Json -Compress | Write-Error; exit 2 }}",
        script
    );

    debug!(script_len = wrapped.len(), "executing powershell");

    let output = Command::new("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &wrapped,
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    debug!(exit_code, stdout_len = stdout.len(), stderr_len = stderr.len(), "powershell completed");

    if !output.status.success() {
        return Err(PsError::Fail {
            command: truncate(script, 200),
            stdout,
            stderr,
            exit_code,
        });
    }

    Ok(PsOutput { stdout, stderr, exit_code })
}

/// Execute a PowerShell script and parse stdout as JSON.
pub fn run_ps_json(script: &str) -> Result<serde_json::Value, PsError> {
    // Ensure ConvertTo-Json is present
    let needs_convert = !script.contains("ConvertTo-Json");
    let body = if needs_convert {
        format!("{script} | ConvertTo-Json -Compress -Depth 5")
    } else {
        script.to_string()
    };

    let output = run_ps(&body)?;

    serde_json::from_str(&output.stdout).map_err(|e| PsError::Json {
        stdout: output.stdout,
        error: e.to_string(),
    })
}

/// Execute with exponential backoff retry for transient errors.
pub fn run_ps_json_retry(
    script: &str,
    max_retries: usize,
    base_delay: Duration,
) -> Result<serde_json::Value, PsError> {
    let mut attempt = 0;
    loop {
        match run_ps_json(script) {
            Ok(v) => return Ok(v),
            Err(e) if e.is_transient() && attempt < max_retries => {
                let delay = base_delay * 2u32.pow(attempt as u32);
                warn!(attempt, ?delay, "transient PS error, retrying");
                std::thread::sleep(delay);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Sanitize a string for safe use in PowerShell single-quoted strings.
/// Escapes single quotes by doubling them (`'` → `''`).
pub fn sanitize_ps_string(s: &str) -> String {
    s.replace('\'', "''")
}

/// Validate a VM name is safe (alphanumeric, hyphens, underscores, spaces).
pub fn validate_vm_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("VM name cannot be empty".into());
    }
    if name.len() > 100 {
        return Err("VM name too long (max 100 chars)".into());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ') {
        return Err(format!(
            "VM name '{}' contains invalid characters (only alphanumeric, hyphens, underscores, spaces)",
            name
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification() {
        assert!(classify_transient("The virtual machine is not ready for use"));
        assert!(classify_transient("A remote session might have ended"));
        assert!(!classify_transient("Access is denied"));
        assert!(!classify_transient("File not found"));
    }

    #[test]
    fn sanitize_quotes() {
        assert_eq!(sanitize_ps_string("test-vm"), "test-vm");
        assert_eq!(sanitize_ps_string("it's a test"), "it''s a test");
        assert_eq!(sanitize_ps_string("a'b'c"), "a''b''c");
    }

    #[test]
    fn validate_vm_names() {
        assert!(validate_vm_name("driver-test-vm").is_ok());
        assert!(validate_vm_name("my vm 123").is_ok());
        assert!(validate_vm_name("").is_err());
        assert!(validate_vm_name("bad;name").is_err());
        assert!(validate_vm_name("$(evil)").is_err());
    }
}
