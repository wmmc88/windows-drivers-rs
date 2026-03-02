# Tasks: Driver Testing CLI Toolset

**Feature Branch**: `001-driver-test-tools`  
**Generated**: November 13, 2025  
**Status**: ✅ Implementation Complete (All Phases 1-7)

---

## Task Organization

Tasks are organized by implementation phases aligned with user stories from spec.md. Each user story represents an independently testable increment of functionality.

**User Story Mapping**:
- **Phase 1 (Setup)**: Infrastructure & foundational components (no user story)
- **Phase 2 (Foundational)**: Core building blocks used across all stories (no user story)
- **Phase 3 (US1)**: Quick Driver Load and Verify (Priority P1)
- **Phase 4 (US2)**: Debug Output Validation (Priority P2)
- **Phase 5 (US3)**: End-to-End Application Testing (Priority P3)
- **Phase 6 (US4)**: Cross-Repository Testing (Priority P4)
- **Phase 7 (Polish)**: Cross-cutting concerns & documentation

---

## Phase 1: Setup (Project Initialization)

**Goal**: Create project structure, configure dependencies, establish development workflow

**Independent Test**: Project builds with `cargo build`, tests run with `cargo test`, help displays with `cargo run -- --help`

### Tasks

- [x] T001 Create Cargo workspace at tools/driver-test-cli/
- [x] T002 Configure Cargo.toml with dependencies (clap, thiserror, serde, tracing, windows)
- [x] T003 Create module structure in src/ (cli, config, vm, driver_detect, package, deploy, debug, echo_test, output, errors, ps, lib)
- [x] T004 Implement tracing initialization in src/main.rs with verbosity levels (-v, -vv, -vvv)
- [x] T005 [P] Create README.md with build instructions and prerequisites
- [x] T006 [P] Create docs/parser-notes.md for implementation documentation
- [x] T007 [P] Configure CI linting (clippy, rustfmt) in .github/workflows/

---

## Phase 2: Foundational (Blocking Prerequisites)

**Goal**: Core infrastructure that all user stories depend on

**Independent Test**: Unit tests pass for error handling, PowerShell wrapper, and driver detection

**MUST COMPLETE BEFORE USER STORIES**: These components are prerequisites for all subsequent phases

### Tasks

- [x] T008 Define AppError taxonomy in src/errors.rs with Error, VmError, DeployError, DetectionError variants
- [x] T009 Implement PowerShell JSON wrapper in src/ps.rs (run_ps_json function with error handling)
- [x] T010 [P] Implement DriverDetector trait in src/driver_detect.rs
- [x] T011 [P] Add INF parser for driver type detection in src/driver_detect.rs
- [x] T012 [P] Implement repository type detection (windows-drivers-rs vs Windows-Rust-driver-samples) in src/driver_detect.rs
- [x] T013 Implement VmProvider trait in src/vm.rs
- [x] T014 Implement HypervProvider struct in src/vm.rs with PowerShell Direct integration
- [x] T015 [P] Create DriverPackage struct in src/package.rs with validation
- [x] T016 [P] Create TestConfiguration struct in src/config.rs with TOML loading
- [x] T017 [P] Add unit tests for INF parsing in tests/detect_driver.rs
- [x] T018 [P] Add unit tests for PowerShell wrapper error handling in tests/ps_wrapper.rs
- [x] T019 [P] Add unit tests for AppError conversion chain in tests/errors.rs

---

## Phase 3: User Story 1 - Quick Driver Load and Verify (P1)

**Goal**: Enable single-command driver deployment and verification workflow

**User Story**: A developer working on a windows-drivers-rs example wants to test their driver changes quickly. They run a single command from within their driver package directory, and the tool automatically detects the driver package, deploys it to a test VM (creating or reusing one), installs it with the test certificate, and verifies the driver is loaded with the exact version built.

**Independent Test**: Build sample-kmdf-driver, run `driver-test` from that directory, verify driver loads in VM with matching version. Success = "cargo build" to "driver verified loaded" with one command in <5min.

**Acceptance Criteria** (from spec.md US1):
1. Detect driver package type (KMDF/UMDF/WDM) from cargo directory
2. Create new Hyper-V VM if none exists (following windows-drivers-rs guidelines)
3. Reuse existing properly configured test VM
4. Copy driver package and test certificate to VM
5. Install test certificate and load driver
6. Verify exact version match and PnP device status

### Tasks

- [x] T020 [US1] Implement detect_driver_package function in src/driver_detect.rs with Cargo.toml parsing
- [x] T021 [US1] Implement locate_build_output in src/package.rs to find driver artifacts (SYS, INF, CAT, CER)
- [x] T022 [US1] Implement create_vm in src/vm.rs with VmConfiguration parameter
- [x] T023 [US1] Implement get_vm in src/vm.rs with VM detection and reuse logic
- [x] T024 [US1] Implement copy_file in src/vm.rs using Copy-VMFile PowerShell command
- [x] T025 [US1] Implement execute_command in src/vm.rs using PowerShell Direct (Invoke-Command)
- [x] T026 [US1] Implement install_test_certificate in src/deploy.rs with certmgr.exe wrapper
- [x] T027 [US1] Implement check_certificate_installed in src/deploy.rs to avoid duplicate installs
- [x] T028 [US1] Implement install_driver in src/deploy.rs using pnputil /add-driver
- [x] T029 [US1] Implement parse_pnputil_enum_output in src/deploy.rs for driver enumeration
- [x] T030 [US1] Implement verify_driver_version in src/deploy.rs with exact version matching
- [x] T031 [US1] Implement query_pnp_device_status in src/deploy.rs for device node validation
- [x] T032 [US1] Create DeployResult struct in src/output.rs with version, status, device_id fields
- [x] T033 [US1] Implement emit_deploy in src/output.rs for human-readable and JSON output
- [x] T034 [US1] Implement DeployCommand in src/cli.rs with clap derive and execute method
- [x] T035 [US1] Add dependency injection for DeployCommand (DriverDeployer trait)
- [x] T036 [US1] Implement SetupCommand in src/cli.rs for VM creation workflow
- [x] T037 [US1] Implement CleanCommand in src/cli.rs for VM removal
- [x] T038 [US1] Add unit tests for pnputil parser in tests/pnputil_parse.rs (single entry, multiple entries, missing fields)
- [x] T039 [US1] Add integration test for deploy command with mock deployer in tests/deploy_integration.rs
- [x] T040 [US1] Add CLI help text test in tests/cli_help.rs
- [x] T041 [US1] Add JSON output failure test in tests/deploy_cli.rs
- [x] T042 [US1] Add end-to-end VM lifecycle test in tests/vm_ops.rs (create, snapshot, revert, cleanup)
- [x] T043 [US1] Update README.md with usage examples for deploy command
- [x] T044 [US1] Document VM configuration requirements in docs/setup-guide.md

---

## Phase 4: User Story 2 - Debug Output Validation (P2)

**Goal**: Enable real-time debug output capture and validation

**User Story**: A developer needs to validate their driver's debug output to ensure logging is working correctly. They run the test with output capture enabled, and the tool monitors DbgPrint/OutputDebugString messages from the driver in real-time, displaying them in the console and optionally validating against expected patterns.

**Independent Test**: Load driver with DbgPrint calls, run `driver-test --capture-output`, verify expected messages appear in console. Success = developers see driver debug messages without manual kernel debugger attachment.

**Acceptance Criteria** (from spec.md US2):
1. Capture DbgPrint and OutputDebugString messages from all driver types
2. Display debug messages in real-time with timestamps
3. Validate against expected output patterns
4. Provide summary of captured output

### Tasks

- [x] T045 [US2] Define DebugMessage struct in src/debug.rs with message_text, timestamp, source, level fields
- [x] T046 [US2] Implement DebugOutputCapture trait in src/debug.rs
- [x] T047 [US2] Implement start_capture in src/debug.rs with DebugView deployment to VM
- [x] T048 [US2] Implement read_messages in src/debug.rs with real-time streaming via PowerShell Direct
- [x] T049 [US2] Implement stop_capture in src/debug.rs with log collection
- [x] T050 [US2] Implement message classification (Info/Warning/Error) in src/debug.rs
- [x] T051 [US2] Implement timestamp parsing and correlation in src/debug.rs
- [x] T052 [US2] Add --capture-output flag to DeployCommand in src/cli.rs
- [x] T053 [US2] Implement validate_output_patterns in src/debug.rs for expected message matching
- [x] T054 [US2] Extend DeployResult in src/output.rs with debug_messages field
- [x] T055 [US2] Add debug output section to JSON and human-readable output in src/output.rs
- [x] T056 [US2] Add unit tests for message classification in tests/debug_capture.rs
- [x] T057 [US2] Add integration test for output capture with mock driver in tests/debug_integration.rs
- [x] T058 [US2] Document debug output capture limitations in docs/debug-output.md (early boot prints)
- [x] T059 [US2] Add troubleshooting guide for debug output issues in docs/troubleshooting.md

---

## Phase 5: User Story 3 - End-to-End Application Testing (P3)

**Goal**: Enable driver-application interaction testing (e.g., echo driver scenario)

**User Story**: A developer working on the echo driver example wants to test both the driver and its companion application. They run the test command, and the tool deploys both the driver and the echo.exe application, runs the application in the VM, captures both driver and application output, and verifies the interaction works as expected.

**Independent Test**: Run against echo driver sample, verify tool deploys both components, runs application, validates echo behavior. Success = complete driver-application scenarios work without manual steps.

**Acceptance Criteria** (from spec.md US3):
1. Detect companion application in package structure
2. Copy both driver and application to VM
3. Run application in VM and capture output
4. Correlate driver and application output by timestamp
5. Validate interaction matches expected behavior

### Tasks

- [x] T060 [US3] Define CompanionApplication struct in src/echo_test.rs with executable_path, expected_patterns
- [x] T061 [US3] Implement locate_companion_application in src/driver_detect.rs
- [x] T062 [US3] Implement copy_application in src/deploy.rs for executable transfer
- [x] T063 [US3] Implement run_application in src/echo_test.rs with process execution in VM
- [x] T064 [US3] Implement capture_application_output in src/echo_test.rs
- [x] T065 [US3] Implement correlate_outputs in src/echo_test.rs for driver-app message matching
- [x] T066 [US3] Implement validate_echo_behavior in src/echo_test.rs for echo-specific validation
- [x] T067 [US3] Add TestCommand to src/cli.rs for full echo test workflow
- [x] T068 [US3] Extend DeployResult in src/output.rs with application_output field
- [x] T069 [US3] Add echo driver integration test in tests/echo_driver_test.rs
- [x] T070 [US3] Document echo driver testing workflow in docs/echo-testing.md
- [x] T071 [US3] Add examples/echo-driver-test/ with sample command invocations

---

## Phase 6: User Story 4 - Cross-Repository Testing (P4)

**Goal**: Support Windows-Rust-driver-samples repository structure

**User Story**: A developer wants to test drivers from the Windows-Rust-driver-samples repository using the same workflow. They run the test tool from within a Windows-Rust-driver-samples driver directory, and the tool automatically adapts to that repository's structure while providing the same testing capabilities.

**Independent Test**: Clone Windows-Rust-driver-samples, navigate to sample driver, run tool. Success = same command works for both repository structures without configuration changes.

**Acceptance Criteria** (from spec.md US4):
1. Detect Windows-Rust-driver-samples repository structure
2. Adapt detection logic for samples repository
3. Follow same workflow as windows-drivers-rs drivers

### Tasks

- [x] T072 [US4] Implement detect_samples_repository in src/driver_detect.rs
- [x] T073 [US4] Extend INF parser in src/driver_detect.rs for samples repository conventions
- [x] T074 [US4] Add repository type to DriverPackage in src/package.rs
- [x] T075 [US4] Implement samples-specific build output location in src/package.rs
- [x] T076 [US4] Add samples repository integration test in tests/samples_repo_test.rs
- [x] T077 [US4] Document repository detection logic in docs/repository-detection.md

---

## Phase 7: Polish & Cross-Cutting Concerns

**Goal**: Production-ready quality, comprehensive documentation, deployment readiness

**Independent Test**: All tests pass, documentation complete, tool installable via cargo install, error messages actionable

### Tasks

- [x] T078 Implement SnapshotCommand in src/cli.rs for baseline snapshot management
- [x] T079 Add --revert-snapshot flag to DeployCommand in src/cli.rs
- [x] T080 Add --rebuild-vm flag to DeployCommand in src/cli.rs
- [x] T081 Implement progress indicators for long operations in src/output.rs
- [x] T082 Add WMI metadata enrichment in src/deploy.rs (Win32_PnPSignedDriver queries)
- [x] T083 Implement log rotation for debug output in src/debug.rs
- [x] T084 Add comprehensive error recovery documentation in docs/error-handling.md
- [x] T085 Create user-guide.md with complete command reference
- [x] T086 Create troubleshooting.md with common issues and solutions
- [x] T087 Add examples/ directory with sample workflows
- [x] T088 Implement exit code standardization (0=success, 1=user error, 2=system error)
- [x] T089 Add performance benchmarks in tests/benchmarks.rs
- [x] T090 Address all clippy warnings and apply rustfmt
- [x] T091 Create release checklist in docs/release.md
- [x] T092 Add CHANGELOG.md with version history
- [x] T093 Configure Cargo.toml for crates.io publication
- [x] T094 Create installation guide in docs/installation.md

---

## Dependencies & Execution Order

### Critical Path (Must Complete in Order)

1. **Phase 1 (Setup)** → **Phase 2 (Foundational)** → **User Stories (Phases 3-6)** → **Phase 7 (Polish)**
2. **Phase 2 MUST complete** before any user story phase begins
3. Within Phase 2: T008-T009 (errors + PowerShell) before T010-T019 (detection + VM operations)
4. User story phases (3-6) are **independent** and can be implemented in parallel after Phase 2

### User Story Dependencies

- **US1 (Phase 3)**: No dependencies beyond Phase 2 (standalone driver deployment)
- **US2 (Phase 4)**: Depends on US1 (requires driver deployment before debug capture)
- **US3 (Phase 5)**: Depends on US1 and US2 (requires both deployment and debug capture)
- **US4 (Phase 6)**: Depends only on Phase 2 (extends detection, uses same deployment)

### Parallel Execution Opportunities

**Phase 1 (Setup)**:
- T005, T006, T007 can run in parallel after T001-T004

**Phase 2 (Foundational)**:
- After T008-T009: T010-T012 (detection), T015-T016 (data structures), T017-T019 (tests) in parallel
- T013-T014 (VM operations) depend on T009 (PowerShell wrapper)

**Phase 3 (US1)**:
- T020-T021 (detection + package) can run in parallel
- T026-T027 (certificate) can run in parallel with T028-T031 (driver install)
- T032-T033 (output) can run in parallel with T034-T037 (CLI)
- T038-T042 (tests) can run in parallel after implementation complete

**Phase 4 (US2)**:
- T045-T051 (debug capture implementation) can run in parallel
- T056-T059 (tests + docs) can run in parallel after implementation

**Phase 5 (US3)**:
- T060-T066 (echo implementation) can run in parallel
- T069-T071 (tests + docs) can run in parallel after implementation

**Phase 6 (US4)**:
- T072-T075 (samples detection) can run in parallel
- T076-T077 (tests + docs) can run in parallel after implementation

**Phase 7 (Polish)**:
- Documentation tasks (T084-T087, T091-T094) can run in parallel
- T082 (WMI enrichment) independent of other tasks
- T089 (benchmarks) independent of other tasks

---

## Implementation Strategy

### MVP Scope (Minimum Viable Product)

**MVP = User Story 1 Only (Phase 3)**

**Rationale**: US1 delivers core value - single-command driver deployment and verification. This alone saves developers significant time on every test iteration.

**MVP Deliverables**:
- Phases 1-2 (Setup + Foundational) - REQUIRED
- Phase 3 (US1) - Core workflow
- Essential polish: Help text, error messages, README

**Post-MVP Increments**:
- Increment 1: +US2 (Debug output) - Adds observability
- Increment 2: +US3 (Application testing) - Completes echo scenario
- Increment 3: +US4 (Cross-repo) - Extends to samples
- Increment 4: Phase 7 (Polish) - Production hardening

### Incremental Delivery Plan

**Week 1**: MVP (Phases 1-3)
- Complete setup and foundational work
- Implement driver deployment workflow
- Basic testing and documentation

**Week 2**: Debug Output (Phase 4)
- Add debug capture capability
- Integration with deployment workflow

**Week 3**: Application Testing (Phase 5)
- Echo driver support
- Driver-application interaction validation

**Week 4**: Cross-Repo + Polish (Phases 6-7)
- Windows-Rust-driver-samples support
- Production hardening
- Comprehensive documentation
- Performance optimization

---

## Metrics & Success Criteria

### Task Completion Metrics

- **Total Tasks**: 94
- **Setup (Phase 1)**: 7 tasks
- **Foundational (Phase 2)**: 12 tasks
- **US1 (Phase 3)**: 25 tasks
- **US2 (Phase 4)**: 15 tasks
- **US3 (Phase 5)**: 12 tasks
- **US4 (Phase 6)**: 6 tasks
- **Polish (Phase 7)**: 17 tasks

### Parallel Execution Potential

- **Phase 1**: 3 parallel tasks (43% parallelizable)
- **Phase 2**: 7 parallel tasks (58% parallelizable)
- **Phase 3**: 15 parallel tasks (60% parallelizable)
- **Phase 4**: 8 parallel tasks (53% parallelizable)
- **Phase 5**: 8 parallel tasks (67% parallelizable)
- **Phase 6**: 4 parallel tasks (67% parallelizable)
- **Phase 7**: 10 parallel tasks (59% parallelizable)

### Success Criteria Mapping

Each user story's success criteria from spec.md:

- **SC-001**: <5min deploy cycle → US1 (Phase 3)
- **SC-002**: <15min VM creation → US1 (Phase 3)
- **SC-003**: 100% VM reuse → US1 (Phase 3)
- **SC-004**: 100% version verification → US1 (Phase 3)
- **SC-005**: 95% debug capture → US2 (Phase 4)
- **SC-006**: <3min app testing → US3 (Phase 5)
- **SC-007**: Cross-repo support → US4 (Phase 6)
- **SC-008**: 100% actionable errors → Phase 7 (Polish)
- **SC-009**: 80% manual step reduction → US1 (Phase 3)
- **SC-010**: <10min full validation → US3 (Phase 5)

---

## Current Status Summary

**Implementation Status**: ✅ **ALL PHASES COMPLETE**

**Completed Phases**:
- ✅ Phase 1: Setup - 7/7 tasks (100%)
- ✅ Phase 2: Foundational - 12/12 tasks (100%)
- ✅ Phase 3: User Story 1 (P1) - 25/25 tasks (100%)
- ✅ Phase 4: User Story 2 (P2) - 15/15 tasks (100%)
- ✅ Phase 5: User Story 3 (P3) - 12/12 tasks (100%)
- ✅ Phase 6: User Story 4 (P4) - 6/6 tasks (100%)
- ✅ Phase 7: Polish - 17/17 tasks (100%)

**Overall Progress**: 94/94 tasks (100% complete)

**Test Results**: `cargo test` (tools/driver-test-cli, 2025-11-16) passes all integration suites.

**Next Steps**: Ready for release (v0.1.0 → v0.2.0 recommended with exit code improvements)

**Next Priority**: Phase 7 polish items (documentation hardening, release prep)
