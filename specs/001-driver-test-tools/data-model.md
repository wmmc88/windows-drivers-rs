# Data Model: Driver Testing CLI Toolset

**Date**: 2025-11-12
**Source**: Extracted from plan.md entities section

---
## Entity: DriverPackage
- **Fields**:
  - `package_name: String`
  - `driver_type: KMDF | UMDF | WDM`
  - `version: String`
  - `architecture: x64 | x86 | arm64`
  - `build_output_path: Path`
  - `inf_path: Path`
  - `certificate_path: Path`
- **Validation**:
  - INF must exist
  - Certificate must be valid (not expired, code signing)
  - Version must be parseable
- **Relationships**:
  - Detected from CargoWorkspace
  - Deployed to TestVM
- **State**:
  - Detected → Built → Validated → Deployed

---
## Entity: TestVM
- **Fields**:
  - `vm_name: String`
  - `vm_id: String`
  - `state: Running | Stopped`
  - `configuration: { memory: u32, cpu: u32, network: String }`
  - `baseline_snapshot_id: String`
  - `installed_certificates: Vec<TestCertificate>`
  - `loaded_drivers: Vec<DriverPackage>`
- **Validation**:
  - Hyper-V VM must exist
  - PowerShell Direct accessible
- **Relationships**:
  - Hosts DriverPackage deployments
  - Captures DebugOutputStream
- **State**:
  - Creating → Configuring → Baseline → Ready → InUse → Cleanup

---
## Entity: TestCertificate
- **Fields**:
  - `thumbprint: String`
  - `subject: String`
  - `expiration_date: DateTime`
  - `installation_status: NotInstalled | Installed | Trusted`
- **Validation**:
  - Not expired
  - Valid for code signing
- **Relationships**:
  - Required by DriverPackage
  - Installed in TestVM
- **State**:
  - Detected → Validated → Installed → Verified

---
## Entity: DebugOutputStream
- **Fields**:
  - `message_text: String`
  - `timestamp: DateTime`
  - `source: Driver | Application`
  - `level: Info | Warning | Error`
- **Validation**:
  - Timestamp ordering
  - Source filter
- **Relationships**:
  - Emitted by DriverPackage or CompanionApplication in TestVM
- **State**:
  - Captured → Filtered → Validated

---
## Entity: CompanionApplication
- **Fields**:
  - `executable_path: Path`
  - `expected_output_patterns: Vec<String>`
  - `interaction_scenarios: Vec<String>`
- **Validation**:
  - Executable exists and is PE format
- **Relationships**:
  - Packaged with DriverPackage
  - Executed in TestVM
- **State**:
  - Detected → Copied → Executed → OutputCaptured

---
## Entity: TestConfiguration
- **Fields**:
  - `vm_identifier: String`
  - `repository_type: WindowsDriversRs | WindowsRustDriverSamples`
  - `validation_rules: Vec<String>`
  - `output_capture_settings: String`
  - `timeout_config: u64`
- **Validation**:
  - VM identifier must resolve to TestVM
  - timeouts > 0
- **Relationships**:
  - Persisted across test runs
  - Applied to TestVM operations
- **State**:
  - Default → Loaded → Applied → Persisted

---
## Notes
- All entities are designed for Rust struct mapping and TOML/JSON serialization.
- Validation rules are enforced at construction and before deployment.
- Relationships are implemented via references or IDs.
- States are tracked via enums or status fields.
