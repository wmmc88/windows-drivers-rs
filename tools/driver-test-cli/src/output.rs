//! Output formatting (JSON and human-readable).

use crate::debug::DebugMessage;
use crate::deploy::{InstallResult, WmiInfo};
use crate::echo::AppOutput;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub driver_type: Option<String>,
    pub published_name: Option<String>,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmi: Option<WmiInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_messages: Option<Vec<DebugMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companion_output: Option<AppOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Emit test result as JSON or human-readable text.
pub fn emit_result(result: &TestResult, json: bool) {
    if json {
        if let Ok(s) = serde_json::to_string_pretty(result) {
            println!("{s}");
        }
    } else {
        if result.success {
            println!("✔ Driver deployed successfully");
            if let Some(dt) = &result.driver_type {
                println!("  Type: {dt}");
            }
            if let Some(name) = &result.published_name {
                println!("  Published: {name}");
            }
            if let Some(ver) = &result.version {
                println!("  Version: {ver}");
            }
            if let Some(wmi) = &result.wmi {
                if let Some(dev) = &wmi.device_name {
                    println!("  Device: {dev}");
                }
                if let Some(mfg) = &wmi.manufacturer {
                    println!("  Manufacturer: {mfg}");
                }
            }
            if let Some(msgs) = &result.debug_messages {
                if !msgs.is_empty() {
                    println!("\n  Debug Output ({} messages):", msgs.len());
                    for msg in msgs.iter().take(20) {
                        let level = format!("{:?}", msg.level).to_ascii_uppercase();
                        let src = msg.source.as_deref().unwrap_or("");
                        println!("    [{level}] {src}: {}", msg.message);
                    }
                    if msgs.len() > 20 {
                        println!("    ... and {} more", msgs.len() - 20);
                    }
                }
            }
            if let Some(app) = &result.companion_output {
                println!("\n  Companion App (exit {}):", app.exit_code);
                if !app.stdout.is_empty() {
                    println!("    {}", app.stdout.trim());
                }
                if !app.missing_patterns.is_empty() {
                    println!("    Missing: {:?}", app.missing_patterns);
                }
            }
        } else {
            println!("✗ Driver deployment failed");
            if let Some(err) = &result.error {
                println!("  Error: {err}");
            }
        }
    }
}

/// Simple progress reporter.
pub struct Progress {
    label: String,
    enabled: bool,
}

impl Progress {
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        let label = label.into();
        if enabled {
            eprintln!("▶ {label}...");
        }
        Self { label, enabled }
    }

    pub fn step(&self, detail: &str) {
        if self.enabled {
            eprintln!("  • {detail}");
        }
    }

    pub fn done(&self, detail: &str) {
        if self.enabled {
            eprintln!("  ✔ {} — {detail}", self.label);
        }
    }

    pub fn fail(&self, detail: &str) {
        if self.enabled {
            eprintln!("  ✗ {} — {detail}", self.label);
        }
    }
}
