//! Driver package detection from Cargo metadata and INF/INX heuristics.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl std::fmt::Display for DriverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverType::Kmdf => write!(f, "KMDF"),
            DriverType::Umdf => write!(f, "UMDF"),
            DriverType::Wdm => write!(f, "WDM"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryType {
    WindowsDriversRs,
    WindowsRustDriverSamples,
}

impl RepositoryType {
    pub fn detect(root: &Path) -> Self {
        let mut current = Some(root);
        while let Some(dir) = current {
            if let Some(name) = dir.file_name().and_then(|s| s.to_str()) {
                if name.eq_ignore_ascii_case("windows-rust-driver-samples") {
                    return RepositoryType::WindowsRustDriverSamples;
                }
            }
            // Check marker files
            for marker in &["samples.json", "sample-list.json", ".samples-root", "samples.yaml"] {
                if dir.join(marker).exists() {
                    return RepositoryType::WindowsRustDriverSamples;
                }
            }
            current = dir.parent();
        }
        RepositoryType::WindowsDriversRs
    }
}

/// Detected driver package info.
#[derive(Debug, Clone)]
pub struct DriverPackage {
    pub root: PathBuf,
    pub driver_type: DriverType,
    pub version: Option<String>,
    pub inf_path: Option<PathBuf>,
    pub repository: RepositoryType,
}

#[derive(Debug, Error)]
pub enum DetectionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("no driver package found in {0}")]
    NotFound(String),
}

/// Detect the driver type for a package at the given root.
///
/// Priority: explicit override > Cargo metadata > INF/INX scan > kernel heuristics.
pub fn detect_driver(root: &Path, override_type: Option<&str>) -> Result<DriverPackage, DetectionError> {
    let repository = RepositoryType::detect(root);

    // 1. Explicit override
    if let Some(o) = override_type {
        return Ok(DriverPackage {
            root: root.to_path_buf(),
            driver_type: parse_driver_type(o),
            version: None,
            inf_path: None,
            repository,
        });
    }

    // 2. Cargo.toml metadata
    let cargo_path = root.join("Cargo.toml");
    if cargo_path.exists() {
        let content = fs::read_to_string(&cargo_path)?;

        if let Some(dt) = scan_cargo_metadata(&content) {
            return Ok(DriverPackage {
                root: root.to_path_buf(),
                driver_type: dt,
                version: None,
                inf_path: find_inf(root),
                repository,
            });
        }

        // 3. Kernel heuristic fallback
        if is_kernel_like(&content) {
            return Ok(DriverPackage {
                root: root.to_path_buf(),
                driver_type: DriverType::Wdm,
                version: None,
                inf_path: find_inf(root),
                repository,
            });
        }
    }

    // 4. INF/INX scanning
    if let Some((dt, version, inf_path)) = scan_inf_files(root) {
        return Ok(DriverPackage {
            root: root.to_path_buf(),
            driver_type: dt,
            version,
            inf_path: Some(inf_path),
            repository,
        });
    }

    Err(DetectionError::NotFound(root.display().to_string()))
}

fn parse_driver_type(s: &str) -> DriverType {
    match s.to_ascii_uppercase().as_str() {
        "KMDF" => DriverType::Kmdf,
        "UMDF" => DriverType::Umdf,
        _ => DriverType::Wdm,
    }
}

/// Look for `driver-type = "KMDF|UMDF|WDM"` in Cargo.toml content.
fn scan_cargo_metadata(content: &str) -> Option<DriverType> {
    let re = regex::Regex::new(r#"(?i)driver-type\s*=\s*"(KMDF|UMDF|WDM)""#).ok()?;
    let caps = re.captures(content)?;
    Some(parse_driver_type(&caps[1]))
}

fn is_kernel_like(cargo_content: &str) -> bool {
    cargo_content.contains(r#"panic = "abort""#) && {
        // Also check for no_std in lib.rs nearby
        // This is a weak heuristic — Cargo.toml alone can hint
        cargo_content.contains("wdk-panic") || cargo_content.contains("wdk-alloc")
    }
}

/// Find first INF or INX file under root.
fn find_inf(root: &Path) -> Option<PathBuf> {
    for entry in WalkDir::new(root).max_depth(5).into_iter().filter_map(|e| e.ok()) {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("inf") || ext.eq_ignore_ascii_case("inx") {
                return Some(entry.into_path());
            }
        }
    }
    None
}

/// Scan INF/INX files for driver type indicators and version.
fn scan_inf_files(root: &Path) -> Option<(DriverType, Option<String>, PathBuf)> {
    for entry in WalkDir::new(root).max_depth(5).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str())?.to_ascii_lowercase();
        if ext != "inf" && ext != "inx" {
            continue;
        }

        let content = read_inf_content(path).ok()?;
        let lower = content.to_ascii_lowercase();

        let dt = if lower.contains("[kmdf]") || lower.contains("kmdflibraryversion") {
            DriverType::Kmdf
        } else if lower.contains("[umdf]") || lower.contains("umdflibraryversion") || lower.contains("umdfservice") {
            DriverType::Umdf
        } else if lower.contains("[version]") {
            DriverType::Wdm
        } else {
            continue;
        };

        let version = extract_driver_ver(&lower);
        return Some((dt, version, path.to_path_buf()));
    }
    None
}

/// Read INF content, handling both UTF-8 and UTF-16LE (common for .inx files).
fn read_inf_content(path: &Path) -> Result<String, DetectionError> {
    let bytes = fs::read(path)?;
    // Check for UTF-16LE BOM (0xFF 0xFE)
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let u16_iter = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
        Ok(String::from_utf16_lossy(&u16_iter.collect::<Vec<u16>>()))
    } else {
        String::from_utf8(bytes).map_err(|_| {
            DetectionError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid UTF-8",
            ))
        })
    }
}

/// Extract version from `DriverVer=date,version` directive.
fn extract_driver_ver(inf_lower: &str) -> Option<String> {
    for line in inf_lower.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("driverver=") || trimmed.starts_with("driverver =") {
            if let Some((_, rest)) = trimmed.split_once(',') {
                let ver = rest.trim();
                if !ver.is_empty() {
                    return Some(ver.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_types() {
        assert_eq!(parse_driver_type("KMDF"), DriverType::Kmdf);
        assert_eq!(parse_driver_type("umdf"), DriverType::Umdf);
        assert_eq!(parse_driver_type("wdm"), DriverType::Wdm);
        assert_eq!(parse_driver_type("unknown"), DriverType::Wdm);
    }

    #[test]
    fn cargo_metadata_scan() {
        let content = r#"
[package.metadata.wdk.driver-model]
driver-type = "KMDF"
kmdf-version-major = 1
"#;
        assert_eq!(scan_cargo_metadata(content), Some(DriverType::Kmdf));
    }

    #[test]
    fn cargo_metadata_umdf() {
        let content = r#"driver-type = "UMDF""#;
        assert_eq!(scan_cargo_metadata(content), Some(DriverType::Umdf));
    }

    #[test]
    fn cargo_metadata_absent() {
        assert_eq!(scan_cargo_metadata("[package]\nname = \"foo\""), None);
    }

    #[test]
    fn driver_ver_extraction() {
        assert_eq!(
            extract_driver_ver("driverver=01/01/2024,1.0.0.0"),
            Some("1.0.0.0".into())
        );
        assert_eq!(extract_driver_ver("no driver ver here"), None);
    }
}
