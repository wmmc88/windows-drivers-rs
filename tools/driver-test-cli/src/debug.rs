//! Debug output capture via DebugView in the guest VM.

use crate::ps::sanitize_ps_string;
use crate::vm::{TestVm, VmError, VmProvider};
use serde::Serialize;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugLevel {
    Info,
    Warn,
    Error,
    Verbose,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugMessage {
    pub message: String,
    pub level: DebugLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("VM error: {0}")]
    Vm(#[from] VmError),
    #[error("DebugView not running in guest")]
    NotRunning,
    #[error("setup failed: {0}")]
    Setup(String),
}

/// Configuration for debug capture.
pub struct CaptureConfig {
    pub guest_log_path: String,
    pub guest_dbgview_path: String,
    pub max_messages: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            guest_log_path: r"C:\DriverLogs\dbwin.log".into(),
            guest_dbgview_path: r"C:\Tools\Dbgview.exe".into(),
            max_messages: 1000,
        }
    }
}

/// Active capture session.
pub struct CaptureSession {
    pub config: CaptureConfig,
    pub vm_name: String,
    started: Instant,
    last_line_count: usize,
}

impl CaptureSession {
    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

/// Configure the guest VM's debug print filter registry key.
/// Required on modern Windows (Vista+) for DbgPrint output to be visible.
pub fn configure_debug_filter<P: VmProvider>(provider: &P, vm: &TestVm) -> Result<(), CaptureError> {
    info!(vm = %vm.name, "configuring Debug Print Filter registry");
    let safe = sanitize_ps_string(&vm.name);
    let script = format!(
        "Invoke-Command -VMName '{safe}' -ScriptBlock {{ \
            $path = 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Debug Print Filter'; \
            if (-not (Test-Path $path)) {{ New-Item -Path $path -Force | Out-Null }}; \
            Set-ItemProperty -Path $path -Name 'DEFAULT' -Value 0xFFFFFFFF -Type DWord; \
            @{{ configured = $true }} | ConvertTo-Json -Compress \
        }}"
    );
    crate::ps::run_ps_json(&script).map_err(|e| CaptureError::Setup(e.to_string()))?;
    Ok(())
}

/// Start debug capture in the guest VM.
///
/// 1. Ensure DebugView is present in guest
/// 2. Kill any existing instance
/// 3. Launch with /k /g /t /q /accepteula /l
pub fn start_capture<P: VmProvider>(
    provider: &P,
    vm: &TestVm,
    config: CaptureConfig,
) -> Result<CaptureSession, CaptureError> {
    let safe = sanitize_ps_string(&vm.name);
    let safe_dbgview = sanitize_ps_string(&config.guest_dbgview_path);
    let safe_log = sanitize_ps_string(&config.guest_log_path);

    info!(vm = %vm.name, log = %config.guest_log_path, "starting debug capture");

    // Check DebugView exists, kill existing, launch new
    let script = format!(
        "Invoke-Command -VMName '{safe}' -ScriptBlock {{ \
            if (-not (Test-Path '{safe_dbgview}')) {{ \
                throw 'DebugView not found at {safe_dbgview}. Copy it to the guest first.' \
            }}; \
            Get-Process Dbgview -ErrorAction SilentlyContinue | Stop-Process -Force; \
            Start-Sleep -Seconds 1; \
            $logDir = Split-Path '{safe_log}' -Parent; \
            New-Item -ItemType Directory -Path $logDir -Force | Out-Null; \
            Remove-Item '{safe_log}' -ErrorAction SilentlyContinue; \
            Start-Process '{safe_dbgview}' -ArgumentList '/k','/g','/t','/q','/accepteula','/l','{safe_log}' -WindowStyle Hidden; \
            Start-Sleep -Seconds 2; \
            $proc = Get-Process Dbgview -ErrorAction SilentlyContinue; \
            @{{ running = ($null -ne $proc); pid = if($proc){{$proc.Id}}else{{$null}} }} | ConvertTo-Json -Compress \
        }}"
    );

    let result = crate::ps::run_ps_json(&script)
        .map_err(|e| CaptureError::Setup(e.to_string()))?;

    let running = result.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
    if !running {
        return Err(CaptureError::NotRunning);
    }

    debug!(vm = %vm.name, "DebugView started");
    Ok(CaptureSession {
        config,
        vm_name: vm.name.clone(),
        started: Instant::now(),
        last_line_count: 0,
    })
}

/// Read new messages from the capture session.
pub fn read_messages<P: VmProvider>(
    provider: &P,
    vm: &TestVm,
    session: &mut CaptureSession,
) -> Result<Vec<DebugMessage>, CaptureError> {
    let safe = sanitize_ps_string(&vm.name);
    let safe_log = sanitize_ps_string(&session.config.guest_log_path);
    let skip = session.last_line_count;

    let script = format!(
        "Invoke-Command -VMName '{safe}' -ScriptBlock {{ \
            if (Test-Path '{safe_log}') {{ \
                $lines = Get-Content '{safe_log}'; \
                $new = $lines | Select-Object -Skip {skip}; \
                @{{ lines = @($new); total = $lines.Count }} | ConvertTo-Json -Compress -Depth 3 \
            }} else {{ \
                @{{ lines = @(); total = 0 }} | ConvertTo-Json -Compress \
            }} \
        }}"
    );

    let result = crate::ps::run_ps_json(&script)
        .map_err(|e| CaptureError::Setup(e.to_string()))?;

    let total = result.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    session.last_line_count = total;

    let lines = result
        .get("lines")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(classify_message)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(lines)
}

/// Stop debug capture and return all collected messages.
pub fn stop_capture<P: VmProvider>(
    provider: &P,
    vm: &TestVm,
    mut session: CaptureSession,
) -> Result<Vec<DebugMessage>, CaptureError> {
    // Final read
    let mut messages = read_messages(provider, vm, &mut session)?;

    // Stop DebugView
    let safe = sanitize_ps_string(&vm.name);
    let script = format!(
        "Invoke-Command -VMName '{safe}' -ScriptBlock {{ \
            Get-Process Dbgview -ErrorAction SilentlyContinue | Stop-Process -Force; \
            @{{ stopped = $true }} | ConvertTo-Json -Compress \
        }}"
    );
    let _ = crate::ps::run_ps_json(&script);

    // Rotate if needed
    if messages.len() > session.config.max_messages {
        let overflow = messages.len() - session.config.max_messages;
        warn!(overflow, "rotating debug messages, dropping oldest");
        messages.drain(0..overflow);
    }

    info!(
        count = messages.len(),
        elapsed_ms = session.elapsed_ms(),
        "debug capture stopped"
    );
    Ok(messages)
}

/// Classify a debug message line by severity.
pub fn classify_message(line: &str) -> DebugMessage {
    let lower = line.to_ascii_lowercase();
    let level = if lower.contains("error") || lower.contains("fail") || lower.contains("bug") {
        DebugLevel::Error
    } else if lower.contains("warn") || lower.contains("deprecated") {
        DebugLevel::Warn
    } else if lower.contains("verbose") || lower.contains("trace") {
        DebugLevel::Verbose
    } else {
        DebugLevel::Info
    };

    // Extract source: text before first ':'
    let source = line
        .split_once(':')
        .map(|(s, _)| s.trim().to_string())
        .filter(|s| s.len() <= 64 && !s.contains(' '));

    DebugMessage {
        message: line.to_string(),
        level,
        source,
    }
}

/// Validate that expected patterns appear in captured messages.
pub fn validate_patterns(messages: &[DebugMessage], patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter(|p| !messages.iter().any(|m| m.message.contains(p.as_str())))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_levels() {
        assert_eq!(classify_message("MyDriver: init ok").level, DebugLevel::Info);
        assert_eq!(classify_message("MyDriver: error occurred").level, DebugLevel::Error);
        assert_eq!(classify_message("WARNING: low memory").level, DebugLevel::Warn);
        assert_eq!(classify_message("verbose: tracing data").level, DebugLevel::Verbose);
    }

    #[test]
    fn classify_source() {
        let msg = classify_message("MyDriver: hello world");
        assert_eq!(msg.source.as_deref(), Some("MyDriver"));

        let msg = classify_message("no colon here");
        assert_eq!(msg.source, None);

        // Long source gets filtered
        let msg = classify_message(&format!("{}: test", "a".repeat(100)));
        assert_eq!(msg.source, None);
    }

    #[test]
    fn pattern_validation() {
        let messages = vec![
            classify_message("echo: sending packet"),
            classify_message("echo: received packet"),
        ];
        let missing = validate_patterns(&messages, &[
            "echo: sending packet".into(),
            "echo: received packet".into(),
            "echo: timeout".into(),
        ]);
        assert_eq!(missing, vec!["echo: timeout"]);
    }
}
