// POC-8: Driver Detection Heuristics Validation
//
// Scans the windows-drivers-rs example drivers and validates driver-type
// detection through a layered heuristic approach:
//   1. Cargo.toml [package.metadata.wdk.driver-model] driver-type
//   2. INF/INX file scanning (.sys => kernel, .dll => UMDF, UmdfService => UMDF)
//   3. Kernel-like heuristics (panic=abort, no_std)
//
// Self-contained: no external crate dependencies.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
    KernelUnknown, // kernel-mode but can't distinguish KMDF vs WDM
    Unknown,
}

impl std::fmt::Display for DriverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverType::Kmdf => write!(f, "KMDF"),
            DriverType::Umdf => write!(f, "UMDF"),
            DriverType::Wdm => write!(f, "WDM"),
            DriverType::KernelUnknown => write!(f, "Kernel (unknown framework)"),
            DriverType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug)]
struct DetectionResult {
    method: &'static str,
    driver_type: DriverType,
    confidence: &'static str,
    details: String,
}

// ---------------------------------------------------------------------------
// Layer 1 – Cargo.toml metadata
// ---------------------------------------------------------------------------

fn detect_from_cargo_metadata(cargo_toml: &str) -> Option<DetectionResult> {
    // Minimal TOML parser: find [package.metadata.wdk.driver-model] section
    // and extract driver-type value.
    let mut in_section = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == "[package.metadata.wdk.driver-model]";
            continue;
        }
        if in_section {
            if let Some(rest) = trimmed.strip_prefix("driver-type") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let val = rest.trim().trim_matches('"');
                    let dt = match val.to_uppercase().as_str() {
                        "KMDF" => DriverType::Kmdf,
                        "UMDF" => DriverType::Umdf,
                        "WDM" => DriverType::Wdm,
                        _ => DriverType::Unknown,
                    };
                    return Some(DetectionResult {
                        method: "Cargo.toml metadata",
                        driver_type: dt,
                        confidence: "high",
                        details: format!("driver-type = \"{}\"", val),
                    });
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Layer 2 – INF / INX scanning
// ---------------------------------------------------------------------------

fn find_inf_inx_files(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(OsStr::to_str) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "inf" || ext_lower == "inx" {
                        results.push(p);
                    }
                }
            }
        }
    }
    results
}

/// Read a file that may be UTF-16LE (with BOM) or UTF-8.
fn read_text_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    // Check for UTF-16LE BOM (FF FE)
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        // Decode UTF-16LE
        let u16_iter = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]));
        let decoded: String = std::char::decode_utf16(u16_iter)
            .map(|r| r.unwrap_or('\u{FFFD}'))
            .collect();
        Some(decoded)
    } else {
        String::from_utf8(bytes).ok()
    }
}

fn detect_from_inf(dir: &Path) -> Option<DetectionResult> {
    let inf_files = find_inf_inx_files(dir);
    if inf_files.is_empty() {
        return None;
    }

    for inf_path in &inf_files {
        let content = match read_text_file(inf_path) {
            Some(c) => c,
            None => continue,
        };
        let lower = content.to_lowercase();
        let fname = inf_path.file_name().unwrap_or_default().to_string_lossy();

        // UMDF indicators: UmdfService, UmdfLibraryVersion, .dll in SourceDisksFiles
        let has_umdf_service = lower.contains("umdfservice");
        let has_umdf_lib = lower.contains("umdflibraryversion");
        let has_dll = lower.contains(".dll");

        // KMDF indicators: KmdfService, SERVICE_KERNEL_DRIVER with .sys
        let has_sys = lower.contains(".sys");
        let has_kmdf_ref = lower.contains("kmdf");

        // WDM: .sys but no KMDF/UMDF references
        let has_umdf_ref = lower.contains("umdf") || has_umdf_service || has_umdf_lib;

        if has_umdf_service || has_umdf_lib || (has_dll && has_umdf_ref) {
            return Some(DetectionResult {
                method: "INF/INX scan",
                driver_type: DriverType::Umdf,
                confidence: "high",
                details: format!(
                    "{}: UmdfService={}, UmdfLib={}, .dll={}",
                    fname, has_umdf_service, has_umdf_lib, has_dll
                ),
            });
        }

        if has_sys && has_kmdf_ref && !has_umdf_ref {
            return Some(DetectionResult {
                method: "INF/INX scan",
                driver_type: DriverType::Kmdf,
                confidence: "medium",
                details: format!("{}: .sys with KMDF references", fname),
            });
        }

        if has_sys && !has_kmdf_ref && !has_umdf_ref {
            // Could be WDM – kernel driver with no framework markers
            return Some(DetectionResult {
                method: "INF/INX scan",
                driver_type: DriverType::Wdm,
                confidence: "medium",
                details: format!("{}: .sys with no WDF framework references", fname),
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Layer 3 – Kernel-like heuristics from Cargo.toml + source
// ---------------------------------------------------------------------------

fn detect_from_heuristics(dir: &Path, cargo_toml: &str) -> Option<DetectionResult> {
    let mut signals: Vec<String> = Vec::new();

    // Check panic = "abort" in profile sections
    let has_panic_abort = cargo_toml.contains("panic = \"abort\"");
    if has_panic_abort {
        signals.push("panic=abort in profile".into());
    }

    // Check for no_std in lib.rs
    let lib_rs = dir.join("src").join("lib.rs");
    let has_no_std = if let Ok(src) = fs::read_to_string(&lib_rs) {
        src.contains("#![no_std]")
    } else {
        false
    };
    if has_no_std {
        signals.push("#![no_std] in lib.rs".into());
    }

    // Check for kernel-crate dependencies
    let has_wdk_panic = cargo_toml.contains("wdk-panic");
    let has_wdk_alloc = cargo_toml.contains("wdk-alloc");
    if has_wdk_panic {
        signals.push("depends on wdk-panic".into());
    }
    if has_wdk_alloc {
        signals.push("depends on wdk-alloc".into());
    }

    // Check crate-type
    let has_cdylib = cargo_toml.contains("cdylib");
    if has_cdylib {
        signals.push("crate-type = cdylib".into());
    }

    if signals.is_empty() {
        return None;
    }

    // Decide: if no_std + panic=abort => kernel-mode (KMDF or WDM, can't tell)
    // if cdylib but NO no_std => likely UMDF
    let driver_type = if has_no_std && has_panic_abort {
        DriverType::KernelUnknown
    } else if has_cdylib && !has_no_std {
        DriverType::Umdf
    } else if has_panic_abort || has_no_std {
        DriverType::KernelUnknown
    } else {
        DriverType::Unknown
    };

    Some(DetectionResult {
        method: "Heuristics",
        driver_type,
        confidence: "low",
        details: signals.join(", "),
    })
}

// ---------------------------------------------------------------------------
// Combined detection
// ---------------------------------------------------------------------------

fn detect_driver(dir: &Path) -> Vec<DetectionResult> {
    let mut results = Vec::new();

    let cargo_path = dir.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_path).unwrap_or_default();

    // Layer 1: Cargo.toml metadata (highest confidence)
    if let Some(r) = detect_from_cargo_metadata(&cargo_toml) {
        results.push(r);
    }

    // Layer 2: INF/INX scanning
    if let Some(r) = detect_from_inf(dir) {
        results.push(r);
    }

    // Layer 3: Heuristics
    if let Some(r) = detect_from_heuristics(dir, &cargo_toml) {
        results.push(r);
    }

    results
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn expected_types() -> HashMap<&'static str, DriverType> {
    let mut m = HashMap::new();
    m.insert("sample-kmdf-driver", DriverType::Kmdf);
    m.insert("sample-umdf-driver", DriverType::Umdf);
    m.insert("sample-wdm-driver", DriverType::Wdm);
    m
}

fn is_compatible(detected: &DriverType, expected: &DriverType) -> bool {
    if detected == expected {
        return true;
    }
    // KernelUnknown is compatible with both KMDF and WDM
    if *detected == DriverType::KernelUnknown {
        return *expected == DriverType::Kmdf || *expected == DriverType::Wdm;
    }
    false
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    println!("=== POC-8: Driver Detection Heuristics Validation ===");
    println!();

    // Determine the examples directory relative to this binary or via env var.
    let examples_dir = if let Ok(dir) = std::env::var("EXAMPLES_DIR") {
        PathBuf::from(dir)
    } else {
        // Default: look relative to the repo worktree
        let candidates = [
            PathBuf::from(r"D:\git-repos\github\windows-drivers-rs.git\driver-test-tool-v2\examples"),
            PathBuf::from("examples"),
            PathBuf::from(r"..\..\..\examples"),
        ];
        match candidates.iter().find(|p| p.is_dir()) {
            Some(p) => p.clone(),
            None => {
                eprintln!("ERROR: Cannot find examples directory.");
                eprintln!("Set EXAMPLES_DIR environment variable to the path containing sample drivers.");
                std::process::exit(1);
            }
        }
    };

    println!("Examples dir: {}", examples_dir.display());
    println!();

    let expected = expected_types();
    let driver_dirs = ["sample-kmdf-driver", "sample-umdf-driver", "sample-wdm-driver"];

    let mut total = 0u32;
    let mut pass = 0u32;
    let mut fail = 0u32;

    for name in &driver_dirs {
        let dir = examples_dir.join(name);
        if !dir.is_dir() {
            println!("[SKIP] {} — directory not found", name);
            continue;
        }

        println!("--- {} ---", name);
        let results = detect_driver(&dir);

        if results.is_empty() {
            println!("  No detection results!");
        }

        let expected_type = expected.get(name.to_owned()).unwrap_or(&DriverType::Unknown);

        for r in &results {
            let compat = is_compatible(&r.driver_type, expected_type);
            let status = if compat { "OK" } else { "MISMATCH" };
            println!(
                "  [{}] {} => {} (confidence: {}, expected: {})",
                status, r.method, r.driver_type, r.confidence, expected_type
            );
            println!("         details: {}", r.details);

            // Count only the highest-confidence layer for pass/fail
        }

        // Use the first (highest-priority) result for the overall verdict
        total += 1;
        if let Some(primary) = results.first() {
            if is_compatible(&primary.driver_type, expected_type) {
                pass += 1;
                println!("  VERDICT: PASS (primary: {} via {})", primary.driver_type, primary.method);
            } else {
                fail += 1;
                println!(
                    "  VERDICT: FAIL (primary: {} via {}, expected: {})",
                    primary.driver_type, primary.method, expected_type
                );
            }
        } else {
            fail += 1;
            println!("  VERDICT: FAIL (no detection)");
        }
        println!();
    }

    println!("=== Summary ===");
    println!("Total: {}  Pass: {}  Fail: {}", total, pass, fail);

    if fail > 0 {
        std::process::exit(1);
    } else {
        println!("All detections correct!");
    }
}
