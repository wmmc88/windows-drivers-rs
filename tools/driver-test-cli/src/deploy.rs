//! Driver deployment: certificate install, pnputil, version verification.

use crate::ps::{run_ps_json, sanitize_ps_string, PsError};
use crate::vm::{CommandOutput, TestVm, VmError, VmProvider};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum DeployError {
    #[error("certificate install failed: {0}")]
    Cert(String),
    #[error("driver install failed: {0}")]
    Driver(String),
    #[error("version mismatch: expected {expected}, found {found}")]
    Version { expected: String, found: String },
    #[error("VM error: {0}")]
    Vm(#[from] VmError),
    #[error("PowerShell error: {0}")]
    Ps(#[from] PsError),
}

/// Result of a successful driver installation.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub inf_used: String,
    pub published_name: Option<String>,
    pub version: Option<String>,
}

/// WMI-enriched driver metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WmiInfo {
    pub device_name: Option<String>,
    pub manufacturer: Option<String>,
    pub driver_provider: Option<String>,
    pub is_signed: Option<bool>,
    pub signer: Option<String>,
}

/// Parsed driver entry from pnputil XML or CIM output.
#[derive(Debug, Clone, Serialize)]
pub struct DriverInfo {
    pub published_name: Option<String>,
    pub provider: Option<String>,
    pub class: Option<String>,
    pub driver_date: Option<String>,
    pub driver_version: Option<String>,
    pub signer_name: Option<String>,
}

/// Trait for driver deployment, enabling mock implementations.
pub trait DriverDeployer {
    fn install_certificate(&self, vm: &TestVm, cert: &Path) -> Result<(), DeployError>;
    fn install_driver(&self, vm: &TestVm, inf: &Path) -> Result<InstallResult, DeployError>;
    fn verify_version(&self, vm: &TestVm, published_name: &str, expected: &str) -> Result<(), DeployError>;
    fn enum_drivers(&self, vm: &TestVm) -> Result<Vec<DriverInfo>, DeployError>;
}

/// Production deployer using pnputil via PowerShell Direct.
pub struct PnpDeployer;

impl Default for PnpDeployer {
    fn default() -> Self {
        Self
    }
}

impl DriverDeployer for PnpDeployer {
    fn install_certificate(&self, vm: &TestVm, cert: &Path) -> Result<(), DeployError> {
        let cert_name = cert
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cert.cer");
        let guest_cert = format!("C:\\DriverTest\\{}", sanitize_ps_string(cert_name));
        let safe_vm = sanitize_ps_string(&vm.name);

        info!(vm = %vm.name, cert = %cert.display(), "installing certificate");

        // Install to both TrustedPeople and Root stores
        let script = format!(
            "Invoke-Command -VMName '{safe_vm}' -ScriptBlock {{ \
                Import-Certificate -FilePath '{guest_cert}' -CertStoreLocation Cert:\\LocalMachine\\TrustedPeople | Out-Null; \
                Import-Certificate -FilePath '{guest_cert}' -CertStoreLocation Cert:\\LocalMachine\\Root | Out-Null; \
                @{{ success = $true }} | ConvertTo-Json -Compress \
            }}"
        );
        run_ps_json(&script).map_err(|e| DeployError::Cert(e.to_string()))?;
        Ok(())
    }

    fn install_driver(&self, vm: &TestVm, inf: &Path) -> Result<InstallResult, DeployError> {
        let inf_name = inf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("driver.inf");
        let guest_inf = format!("C:\\DriverTest\\{}", sanitize_ps_string(inf_name));
        let safe_vm = sanitize_ps_string(&vm.name);

        info!(vm = %vm.name, inf = %inf.display(), "installing driver");

        let script = format!(
            "Invoke-Command -VMName '{safe_vm}' -ScriptBlock {{ \
                $output = pnputil /add-driver '{guest_inf}' /install 2>&1 | Out-String; \
                @{{ output = $output; exitCode = $LASTEXITCODE }} | ConvertTo-Json -Compress \
            }}"
        );

        let result = run_ps_json(&script).map_err(|e| DeployError::Driver(e.to_string()))?;
        let output_text = result
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Extract published name from pnputil output
        let published = extract_published_name(output_text);

        Ok(InstallResult {
            inf_used: inf_name.to_string(),
            published_name: published,
            version: None,
        })
    }

    fn verify_version(
        &self,
        vm: &TestVm,
        published_name: &str,
        expected: &str,
    ) -> Result<(), DeployError> {
        let drivers = self.enum_drivers(vm)?;
        for d in &drivers {
            if d.published_name.as_deref() == Some(published_name) {
                if let Some(ver) = &d.driver_version {
                    if ver == expected {
                        return Ok(());
                    }
                    return Err(DeployError::Version {
                        expected: expected.to_string(),
                        found: ver.clone(),
                    });
                }
            }
        }
        Err(DeployError::Version {
            expected: expected.to_string(),
            found: "not found".to_string(),
        })
    }

    fn enum_drivers(&self, vm: &TestVm) -> Result<Vec<DriverInfo>, DeployError> {
        let safe_vm = sanitize_ps_string(&vm.name);

        // Try XML format first (consensus: more stable than text)
        let script = format!(
            "Invoke-Command -VMName '{safe_vm}' -ScriptBlock {{ \
                $xml = pnputil /enum-drivers /format xml 2>&1 | Out-String; \
                if ($LASTEXITCODE -eq 0 -and $xml -match '<') {{ \
                    @{{ format = 'xml'; data = $xml }} | ConvertTo-Json -Compress \
                }} else {{ \
                    $text = pnputil /enum-drivers 2>&1 | Out-String; \
                    @{{ format = 'text'; data = $text }} | ConvertTo-Json -Compress \
                }} \
            }}"
        );

        let result = run_ps_json(&script).map_err(|e| DeployError::Driver(e.to_string()))?;
        let format = result.get("format").and_then(|v| v.as_str()).unwrap_or("text");
        let data = result.get("data").and_then(|v| v.as_str()).unwrap_or("");

        if format == "xml" {
            Ok(parse_pnputil_xml(data))
        } else {
            Ok(parse_pnputil_text(data))
        }
    }
}

/// Extract published driver name from pnputil /add-driver output.
fn extract_published_name(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim().to_ascii_lowercase();
        if trimmed.contains("published name") || trimmed.contains("nombre publicado") {
            if let Some(idx) = line.find(':') {
                let val = line[idx + 1..].trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Parse pnputil XML output into driver info entries.
fn parse_pnputil_xml(xml: &str) -> Vec<DriverInfo> {
    // Simple tag-based extraction (no XML crate dependency)
    let mut drivers = Vec::new();
    let mut current: Option<DriverInfo> = None;

    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<DriverPackage") || trimmed == "<DriverStore>" {
            if let Some(d) = current.take() {
                if d.published_name.is_some() || d.driver_version.is_some() {
                    drivers.push(d);
                }
            }
            current = Some(DriverInfo {
                published_name: None,
                provider: None,
                class: None,
                driver_date: None,
                driver_version: None,
                signer_name: None,
            });
        }
        if let Some(ref mut d) = current {
            if let Some(val) = extract_xml_value(trimmed, "PublishedName") {
                d.published_name = Some(val);
            } else if let Some(val) = extract_xml_value(trimmed, "DriverPackageProvider") {
                d.provider = Some(val);
            } else if let Some(val) = extract_xml_value(trimmed, "ClassName") {
                d.class = Some(val);
            } else if let Some(val) = extract_xml_value(trimmed, "DriverVersion") {
                d.driver_version = Some(val);
            } else if let Some(val) = extract_xml_value(trimmed, "DriverDate") {
                d.driver_date = Some(val);
            } else if let Some(val) = extract_xml_value(trimmed, "SignerName") {
                d.signer_name = Some(val);
            }
        }
    }
    if let Some(d) = current {
        if d.published_name.is_some() || d.driver_version.is_some() {
            drivers.push(d);
        }
    }
    drivers
}

fn extract_xml_value(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    if let Some(start) = line.find(&open) {
        if let Some(end) = line.find(&close) {
            let val = &line[start + open.len()..end];
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Fallback text parser for pnputil output (less reliable, for older Windows).
fn parse_pnputil_text(text: &str) -> Vec<DriverInfo> {
    let mut drivers = Vec::new();
    let mut current = DriverInfo {
        published_name: None,
        provider: None,
        class: None,
        driver_date: None,
        driver_version: None,
        signer_name: None,
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if current.published_name.is_some() || current.driver_version.is_some() {
                drivers.push(current);
            }
            current = DriverInfo {
                published_name: None,
                provider: None,
                class: None,
                driver_date: None,
                driver_version: None,
                signer_name: None,
            };
            continue;
        }
        if let Some(idx) = trimmed.find(':') {
            let key = trimmed[..idx].trim().to_ascii_lowercase();
            let val = trimmed[idx + 1..].trim().to_string();
            match key.as_str() {
                k if k.contains("published name") => current.published_name = Some(val),
                k if k.contains("provider") => current.provider = Some(val),
                k if k.contains("class") && !k.contains("guid") => current.class = Some(val),
                k if k.contains("driver version") => current.driver_version = Some(val),
                k if k.contains("driver date") && !k.contains("version") => {
                    current.driver_date = Some(val);
                }
                k if k.contains("signer") => current.signer_name = Some(val),
                _ => {}
            }
        }
    }
    if current.published_name.is_some() || current.driver_version.is_some() {
        drivers.push(current);
    }
    drivers
}

/// Query WMI for enriched driver metadata.
pub fn query_wmi(vm: &TestVm, inf_name: &str) -> Result<Option<WmiInfo>, DeployError> {
    let safe_vm = sanitize_ps_string(&vm.name);
    let safe_inf = sanitize_ps_string(inf_name);
    let script = format!(
        "Invoke-Command -VMName '{safe_vm}' -ScriptBlock {{ \
            $d = Get-CimInstance Win32_PnPSignedDriver | Where-Object {{ $_.InfName -eq '{safe_inf}' }} | Select-Object -First 1; \
            if ($d) {{ \
                @{{ DeviceName=$d.DeviceName; Manufacturer=$d.Manufacturer; DriverProviderName=$d.DriverProviderName; IsSigned=$d.IsSigned; Signer=$d.Signer }} | ConvertTo-Json -Compress \
            }} else {{ 'null' }} \
        }}"
    );
    let result = run_ps_json(&script).map_err(|e| DeployError::Ps(e))?;
    if result.is_null() {
        return Ok(None);
    }
    Ok(Some(WmiInfo {
        device_name: result.get("DeviceName").and_then(|v| v.as_str()).map(String::from),
        manufacturer: result.get("Manufacturer").and_then(|v| v.as_str()).map(String::from),
        driver_provider: result.get("DriverProviderName").and_then(|v| v.as_str()).map(String::from),
        is_signed: result.get("IsSigned").and_then(|v| v.as_bool()),
        signer: result.get("Signer").and_then(|v| v.as_str()).map(String::from),
    }))
}

/// Orchestrate the full deployment workflow.
pub fn deploy_driver<D: DriverDeployer, P: VmProvider>(
    deployer: &D,
    provider: &P,
    vm: &TestVm,
    cert: Option<&Path>,
    inf: &Path,
    expected_version: Option<&str>,
) -> Result<InstallResult, DeployError> {
    // 1. Copy files to guest
    let inf_name = inf.file_name().and_then(|n| n.to_str()).unwrap_or("driver.inf");
    let guest_dir = "C:\\DriverTest\\";

    provider
        .copy_file(vm, inf, &format!("{guest_dir}{inf_name}"))
        .map_err(|e| DeployError::Driver(format!("failed to copy INF: {e}")))?;

    // 2. Install certificate if provided
    if let Some(cert_path) = cert {
        let cert_name = cert_path.file_name().and_then(|n| n.to_str()).unwrap_or("cert.cer");
        provider
            .copy_file(vm, cert_path, &format!("{guest_dir}{cert_name}"))
            .map_err(|e| DeployError::Cert(format!("failed to copy cert: {e}")))?;
        deployer.install_certificate(vm, cert_path)?;
    }

    // 3. Install driver
    let result = deployer.install_driver(vm, inf)?;

    // 4. Verify version if requested
    if let (Some(expected), Some(ref published)) = (expected_version, &result.published_name) {
        deployer.verify_version(vm, published, expected)?;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_published_from_output() {
        let output = "Published Name : oem42.inf\nDriver package added successfully.";
        assert_eq!(
            extract_published_name(output),
            Some("oem42.inf".to_string())
        );
    }

    #[test]
    fn xml_value_extraction() {
        assert_eq!(
            extract_xml_value("<PublishedName>oem1.inf</PublishedName>", "PublishedName"),
            Some("oem1.inf".to_string())
        );
        assert_eq!(extract_xml_value("<Other>val</Other>", "PublishedName"), None);
    }

    #[test]
    fn parse_xml_drivers() {
        let xml = r#"
<DriverStore>
  <DriverPackage>
    <PublishedName>oem1.inf</PublishedName>
    <DriverVersion>1.0.0.0</DriverVersion>
    <ClassName>Net</ClassName>
  </DriverPackage>
  <DriverPackage>
    <PublishedName>oem2.inf</PublishedName>
    <DriverVersion>2.0.0.0</DriverVersion>
  </DriverPackage>
</DriverStore>
"#;
        let drivers = parse_pnputil_xml(xml);
        assert_eq!(drivers.len(), 2);
        assert_eq!(drivers[0].published_name.as_deref(), Some("oem1.inf"));
        assert_eq!(drivers[0].driver_version.as_deref(), Some("1.0.0.0"));
        assert_eq!(drivers[1].published_name.as_deref(), Some("oem2.inf"));
    }

    #[test]
    fn parse_text_fallback() {
        let text = "Published Name : oem1.inf\nDriver Version : 1.0\nClass : Net\n\nPublished Name : oem2.inf\nDriver Version : 2.0\n";
        let drivers = parse_pnputil_text(text);
        assert_eq!(drivers.len(), 2);
        assert_eq!(drivers[0].published_name.as_deref(), Some("oem1.inf"));
    }
}
