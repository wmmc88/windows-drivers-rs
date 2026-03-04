//! Companion application testing (echo driver workflow).

use crate::debug::DebugMessage;
use crate::ps::sanitize_ps_string;
use crate::vm::{TestVm, VmError, VmProvider};
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

const GUEST_APP_DIR: &str = r"C:\DriverTest\Apps";

#[derive(Debug, Clone, Serialize)]
pub struct CompanionApp {
    pub executable_path: PathBuf,
    pub expected_patterns: Vec<String>,
}

impl CompanionApp {
    pub fn file_name(&self) -> String {
        self.executable_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("companion.exe")
            .to_string()
    }

    pub fn guest_path(&self) -> String {
        format!("{}\\{}", GUEST_APP_DIR, self.file_name())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub matched_patterns: Vec<String>,
    pub missing_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Correlation {
    pub pattern: String,
    pub in_app: bool,
    pub in_driver: bool,
}

#[derive(Debug, Error)]
pub enum EchoError {
    #[error("VM error: {0}")]
    Vm(#[from] VmError),
    #[error("companion app failed (exit code {0})")]
    AppFailed(i32),
    #[error("missing expected patterns: {0:?}")]
    MissingPatterns(Vec<String>),
}

/// Copy companion app to guest and execute it.
pub fn run_companion<P: VmProvider>(
    provider: &P,
    vm: &TestVm,
    app: &CompanionApp,
) -> Result<AppOutput, EchoError> {
    // Copy executable
    let guest_dest = format!("{}\\{}", GUEST_APP_DIR, app.file_name());
    info!(vm = %vm.name, exe = %app.file_name(), "copying companion app");
    provider.copy_file(vm, &app.executable_path, &guest_dest)?;

    // Execute in guest
    let safe_vm = sanitize_ps_string(&vm.name);
    let safe_path = sanitize_ps_string(&app.guest_path());
    info!(vm = %vm.name, exe = %app.file_name(), "running companion app");

    let script = format!(
        "Invoke-Command -VMName '{safe_vm}' -ScriptBlock {{ \
            $out = & '{safe_path}' 2>&1 | Out-String; \
            @{{ stdout = $out; exitCode = $LASTEXITCODE }} | ConvertTo-Json -Compress \
        }}"
    );

    let result = crate::ps::run_ps_json(&script)
        .map_err(|e| EchoError::Vm(VmError::Ps(e)))?;

    let stdout = result.get("stdout").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let exit_code = result.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;

    // Match patterns
    let mut matched = Vec::new();
    let mut missing = Vec::new();
    for pattern in &app.expected_patterns {
        if stdout.contains(pattern.as_str()) {
            matched.push(pattern.clone());
        } else {
            missing.push(pattern.clone());
        }
    }

    Ok(AppOutput {
        stdout,
        stderr: String::new(),
        exit_code,
        matched_patterns: matched,
        missing_patterns: missing,
    })
}

/// Correlate app output patterns with driver debug messages.
pub fn correlate(
    debug_messages: &[DebugMessage],
    app_output: &AppOutput,
    patterns: &[String],
) -> Vec<Correlation> {
    patterns
        .iter()
        .map(|p| Correlation {
            pattern: p.clone(),
            in_app: app_output.matched_patterns.contains(p),
            in_driver: debug_messages.iter().any(|m| m.message.contains(p.as_str())),
        })
        .collect()
}

/// Validate that companion app succeeded and all patterns matched.
pub fn validate(output: &AppOutput) -> Result<(), EchoError> {
    if output.exit_code != 0 {
        return Err(EchoError::AppFailed(output.exit_code));
    }
    if !output.missing_patterns.is_empty() {
        return Err(EchoError::MissingPatterns(output.missing_patterns.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_matching() {
        let output = AppOutput {
            stdout: "echo: sending packet\necho: received packet".into(),
            stderr: String::new(),
            exit_code: 0,
            matched_patterns: vec!["echo: sending packet".into(), "echo: received packet".into()],
            missing_patterns: vec![],
        };
        assert!(validate(&output).is_ok());
    }

    #[test]
    fn missing_pattern_fails() {
        let output = AppOutput {
            stdout: "echo: sending packet".into(),
            stderr: String::new(),
            exit_code: 0,
            matched_patterns: vec!["echo: sending packet".into()],
            missing_patterns: vec!["echo: received packet".into()],
        };
        assert!(matches!(validate(&output), Err(EchoError::MissingPatterns(_))));
    }

    #[test]
    fn correlation() {
        let debug_msgs = vec![
            crate::debug::classify_message("echo: sending packet"),
            crate::debug::classify_message("echo: received packet"),
        ];
        let app = AppOutput {
            stdout: "echo: sending packet".into(),
            stderr: String::new(),
            exit_code: 0,
            matched_patterns: vec!["echo: sending packet".into()],
            missing_patterns: vec!["echo: received packet".into()],
        };
        let patterns = vec!["echo: sending packet".into(), "echo: received packet".into()];
        let corr = correlate(&debug_msgs, &app, &patterns);
        assert_eq!(corr.len(), 2);
        assert!(corr[0].in_app && corr[0].in_driver);
        assert!(!corr[1].in_app && corr[1].in_driver);
    }
}
