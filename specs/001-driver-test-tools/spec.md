# Feature Specification: Driver Testing CLI Toolset

**Feature Branch**: `001-driver-test-tools`  
**Created**: November 11, 2025  
**Status**: Draft  
**Input**: User description: "Build a set of Rust CLI tools to help me test changes to the windows-drivers-rs repo. The solution should be able to setup a hyperv test machine for driver testing (per the various readmes in the examples folder in the wdr repo), reuse vms if its detected and configured properly, The solution should also be able to detect a driver package folder given that you're currenlty in a cargo package folder, copy it into the vm, load the driver (and install the test cert if needed), check that the exact driver is loaded (make sure version matches exactly), check that the driver is functioning as expected per pnp, and then somehow be able to validate the dbgprint/outputdebugstring output (regardless if its a kmdf, umdf, or wdm driver). It should also be able to do checking via the drivers in Windows-Rust-driver-samples repo. for example, it should be able to do all the previously mentionned stuff to load the echo driver, and then also copy over the echo exe and see that the output of the exe and driver are as expected."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Quick Driver Load and Verify (Priority: P1)

A developer working on a windows-drivers-rs example wants to test their driver changes quickly. They run a single command from within their driver package directory, and the tool automatically detects the driver package, deploys it to a test VM (creating or reusing one), installs it with the test certificate, and verifies the driver is loaded with the exact version built.

**Why this priority**: This is the core workflow that delivers immediate value - the ability to quickly test a driver build without manual VM setup, file copying, or driver installation steps. This alone would save developers significant time on every test iteration.

**Independent Test**: Can be fully tested by building any driver example (e.g., sample-kmdf-driver), running the test command from that directory, and verifying the driver loads in the VM with matching version information. Success means the developer goes from "cargo build" to "driver verified loaded" with one command.

**Acceptance Scenarios**:

1. **Given** a developer is in a cargo package directory containing a driver, **When** they run the test command, **Then** the tool detects the driver package type (KMDF/UMDF/WDM), builds it if needed, and reports the driver package path
2. **Given** no test VM exists, **When** the tool runs for the first time, **Then** it creates a new Hyper-V VM following the setup guidelines from windows-drivers-rs examples folder, configures it for driver testing, and saves the VM configuration for reuse
3. **Given** a properly configured test VM already exists, **When** the tool runs, **Then** it reuses the existing VM instead of creating a new one
4. **Given** a driver package has been detected, **When** the tool deploys to the VM, **Then** it copies the driver package files and the test signing certificate to the VM
5. **Given** driver files are copied to the VM, **When** the tool installs the driver, **Then** it installs the test certificate (if not already installed) and loads the driver using appropriate installation method (devcon or pnputil)
6. **Given** the driver installation completes, **When** the tool verifies the driver, **Then** it confirms the exact version matches the built driver version, reports driver status via PnP manager, and confirms the driver is functioning (device node present and started)

---

### User Story 2 - Debug Output Validation (Priority: P2)

A developer needs to validate their driver's debug output to ensure logging is working correctly. They run the test with output capture enabled, and the tool monitors DbgPrint/OutputDebugString messages from the driver in real-time, displaying them in the console and optionally validating against expected patterns.

**Why this priority**: Debug output validation is critical for driver development but secondary to basic driver loading. Developers need this to debug issues and verify logging behavior, but they can manually check debugger output if this feature isn't available. This builds on P1 by adding observability.

**Independent Test**: Can be tested by loading a driver known to produce debug output (e.g., adding DbgPrint calls to a sample), running the tool with output capture, and verifying the expected messages appear in the console output. Success means developers can see driver debug messages without attaching a kernel debugger manually.

**Acceptance Scenarios**:

1. **Given** a driver is loaded in the test VM, **When** the tool starts output monitoring, **Then** it captures all DbgPrint and OutputDebugString messages from the driver, regardless of driver type (KMDF/UMDF/WDM)
2. **Given** debug output is being captured, **When** driver events occur, **Then** the tool displays debug messages in real-time in the console with timestamps
3. **Given** expected output patterns are provided, **When** the tool validates output, **Then** it confirms expected messages appear and reports any missing or unexpected output
4. **Given** the test completes, **When** the tool exits, **Then** it provides a summary of captured output and validation results

---

### User Story 3 - End-to-End Application Testing (Priority: P3)

A developer working on the echo driver example wants to test both the driver and its companion application. They run the test command, and the tool deploys both the driver and the echo.exe application, runs the application in the VM, captures both driver and application output, and verifies the interaction works as expected.

**Why this priority**: This is valuable for comprehensive testing of driver-application pairs but represents a more complete scenario. The core value (P1) is driver loading, and debug output (P2) helps with basic validation. Application interaction testing is important for examples like echo driver but is a more advanced use case.

**Independent Test**: Can be tested by running against the echo driver sample, which includes both the driver and a test application. The tool should deploy both, run the application, and validate the expected echo behavior occurs. Success means developers can verify complete driver-application scenarios without manual steps.

**Acceptance Scenarios**:

1. **Given** a driver package includes a companion application, **When** the tool detects the package structure, **Then** it identifies both driver and application components
2. **Given** both driver and application are detected, **When** the tool deploys to the VM, **Then** it copies both the driver package and the application executable
3. **Given** the driver is loaded and application is copied, **When** the tool runs the application, **Then** it executes the application in the VM and captures its output
4. **Given** the application is running, **When** driver-application interaction occurs, **Then** the tool captures both application output and driver debug output, correlating them by timestamp
5. **Given** expected behavior is defined for the driver-application pair, **When** the tool validates results, **Then** it confirms the interaction matches expected behavior (e.g., echo driver echoes application messages correctly)

---

### User Story 4 - Cross-Repository Testing (Priority: P4)

A developer wants to test drivers from the Windows-Rust-driver-samples repository using the same workflow. They run the test tool from within a Windows-Rust-driver-samples driver directory, and the tool automatically adapts to that repository's structure while providing the same testing capabilities.

**Why this priority**: This extends the tool's utility beyond windows-drivers-rs to the samples repository, increasing its value. However, it's less critical than the core workflows because windows-drivers-rs is the primary development target, and manual testing of samples can continue with existing methods.

**Independent Test**: Can be tested by cloning Windows-Rust-driver-samples, navigating to a sample driver (e.g., echo), and running the tool. Success means the same command works for both repository structures without configuration changes.

**Acceptance Scenarios**:

1. **Given** the tool is run from a Windows-Rust-driver-samples driver directory, **When** it detects the repository, **Then** it identifies the repository structure and adapts detection logic accordingly
2. **Given** a Windows-Rust-driver-samples driver is detected, **When** the tool deploys and tests, **Then** it follows the same workflow as windows-drivers-rs drivers (build, deploy, install, verify)
3. **Given** a sample driver includes a companion application, **When** the tool processes it, **Then** it handles the driver-application testing workflow for samples repository structure

---

### Edge Cases

- What happens when the driver package is corrupt or missing required files?
  - Tool should detect missing files early and provide clear error messages before VM deployment
- How does the system handle when Hyper-V is not enabled or available?
  - Tool should check prerequisites and provide actionable instructions to enable Hyper-V
- What happens when VM creation fails due to insufficient resources?
  - Tool should detect resource constraints and provide clear error messages with minimum requirements
- How does the tool handle driver installation failures?
  - Tool should capture installation errors, provide diagnostics from PnP manager, and preserve VM state for debugging
- What happens when the exact driver version cannot be verified?
  - Tool should warn about version mismatch and provide both expected and actual version information
- How does the system handle when debug output capture fails?
  - Tool should continue with other validations and report that debug output was unavailable
- What happens when multiple VMs exist with similar configurations?
  - Tool should detect ambiguity and either use naming conventions or prompt user to select correct VM
- How does the tool handle when a driver is already loaded in the VM?
  - Tool should detect existing driver, offer to unload and replace it, or reuse it if version matches
- What happens when test certificate installation fails due to security policies?
  - Tool should detect certificate installation failures and provide guidance on enabling test signing
- How does the system handle cross-architecture scenarios (x64 host, ARM64 driver)?
  - Tool should validate architecture compatibility and provide clear errors for unsupported scenarios

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect driver package type (KMDF, UMDF, or WDM) from the current cargo package directory structure and metadata
- **FR-002**: System MUST locate driver package build output (driver files, INF, test certificate) from cargo build artifacts
- **FR-003**: System MUST create a Hyper-V test VM following windows-drivers-rs repository setup guidelines when no suitable VM exists
- **FR-004**: System MUST detect and reuse existing Hyper-V VMs that are properly configured for driver testing
- **FR-005**: System MUST persist VM configuration information to enable reuse across test runs
- **FR-006**: System MUST copy driver package files (driver binary, INF, catalog, test certificate) from host to VM
- **FR-007**: System MUST install test signing certificate in the VM's trusted certificate store before driver installation
- **FR-008**: System MUST skip certificate installation if the same certificate is already installed and trusted
- **FR-009**: System MUST load the driver in the VM using appropriate installation tools (devcon, pnputil, or driver installation API)
- **FR-010**: System MUST verify the loaded driver version matches the exact version that was built
- **FR-011**: System MUST query PnP manager to confirm driver device node is present and in started state
- **FR-012**: System MUST capture DbgPrint output from kernel-mode drivers (KMDF, WDM)
- **FR-013**: System MUST capture OutputDebugString output from user-mode drivers (UMDF)
- **FR-014**: System MUST display captured debug output in real-time to the console
- **FR-015**: System MUST support validation of debug output against expected message patterns
- **FR-016**: System MUST detect when a cargo package includes a companion application (e.g., echo.exe)
- **FR-017**: System MUST copy companion applications to the VM alongside driver packages
- **FR-018**: System MUST execute companion applications in the VM and capture their console output
- **FR-019**: System MUST correlate driver debug output with application output using timestamps
- **FR-020**: System MUST validate driver-application interactions against expected behavior patterns
- **FR-021**: System MUST adapt to windows-drivers-rs repository structure for driver detection
- **FR-022**: System MUST adapt to Windows-Rust-driver-samples repository structure for driver detection
- **FR-023**: System MUST provide clear error messages when prerequisites (Hyper-V, virtualization support) are missing
- **FR-024**: System MUST validate sufficient system resources (memory, disk, CPU) before VM creation
- **FR-025**: System MUST capture and report driver installation failures with diagnostic information from Windows
- **FR-026**: System MUST preserve VM state when errors occur to enable manual debugging
- **FR-027**: System MUST detect and report version mismatches between expected and loaded driver versions
- **FR-028**: System MUST provide actionable guidance when test signing prerequisites are not met
- **FR-029**: System MUST validate architecture compatibility between host, VM, and driver before deployment
- **FR-030**: System MUST offer to unload existing driver versions when deploying a new version of the same driver

### Key Entities

- **Driver Package**: Represents a buildable driver project containing driver binary, INF file, catalog, and test certificate; includes metadata such as driver type (KMDF/UMDF/WDM), version, architecture, and package name
- **Test VM**: Represents a Hyper-V virtual machine configured for driver testing; includes state information (running/stopped), configuration (memory, CPU, network), installed certificates, and currently loaded drivers
- **Test Certificate**: Represents the code signing certificate used for test-signed drivers; includes certificate thumbprint, expiration date, and installation status in VM
- **Debug Output Stream**: Represents captured debug messages from driver execution; includes message text, timestamp, source (driver or application), and message level
- **Companion Application**: Represents an executable that interacts with the driver; includes executable path, expected output patterns, and interaction scenarios
- **Test Configuration**: Represents saved settings for VM reuse and test preferences; includes VM identifier, repository type, default validation rules, and output capture settings

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers can go from "cargo build" to "driver verified loaded in VM" with a single command in under 5 minutes (excluding first-time VM creation)
- **SC-002**: First-time VM creation completes in under 15 minutes with automated configuration following repository guidelines
- **SC-003**: VM reuse detection works correctly 100% of the time for properly configured VMs, avoiding duplicate VM creation
- **SC-004**: Driver version verification correctly identifies version mismatches in 100% of test cases
- **SC-005**: Debug output capture successfully captures all DbgPrint/OutputDebugString messages for all three driver types (KMDF, UMDF, WDM) in 95% of test runs
- **SC-006**: Driver-application interaction testing (e.g., echo driver scenario) completes successfully in under 3 minutes after VM is ready
- **SC-007**: Tool correctly adapts to both windows-drivers-rs and Windows-Rust-driver-samples repository structures without user configuration
- **SC-008**: Error messages provide actionable next steps in 100% of failure scenarios (missing prerequisites, installation failures, resource constraints)
- **SC-009**: Test runs reduce manual steps by at least 80% compared to current manual driver testing workflow
- **SC-010**: Developers can validate complete driver functionality (load, verify, debug output, application interaction) in under 10 minutes for iterative testing

## Clarifications

### Session 2025-11-12

- Q: What default VM lifecycle management strategy should the tool adopt for test runs? → A: Hybrid persistent reuse with a maintained baseline snapshot; reuse current state by default and support flags to revert to baseline or force fresh rebuild (Option D).
- Q: What mechanism should be the default for executing commands and transferring files into the test VM? → A: PowerShell Direct only (Option A) – prioritize simplicity and speed for local host–guest scenarios; no WinRM fallback assumed.

### Adjustments Based on Clarification

Added explicit functional requirement for VM lifecycle management hybrid strategy.

### Updated Functional Requirements (addendum)

- **FR-031**: System MUST implement a hybrid VM lifecycle strategy: maintain a persistent test VM with a stored baseline snapshot; default to reusing the current VM state for speed; provide flags to (a) revert to baseline snapshot before a run and (b) force a full rebuild of the VM environment.
- **FR-032**: System MUST use PowerShell Direct as the sole default channel for in-guest command execution and file transfer when host and VM share the same Hyper-V host; MUST fail with a clear error advising manual network-based setup (e.g., enabling WinRM) if PowerShell Direct is unavailable.
