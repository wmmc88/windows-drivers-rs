# Agent Context (Phase 1)

## Summary
The driver-test-cli crate (v0.1.0 scaffold) exists under `tools/driver-test-cli` with initial module stubs and working CLI entrypoint. Research decisions (R1-R8) implemented via dependency selection and architecture boundaries.

## Key Decisions Embedded
- CLI: clap derive (multi-command structure) ✔
- Runtime: synchronous (external process oriented) ✔
- Logging: tracing (span naming: vm.*, deploy.*, detect.*, ps.*) ✔
- Config: TOML (loader stub) ✔
- Detection: metadata + INF fallback (skeleton implemented) ✔
- PowerShell interop: wrapper `run_ps_json` ✔
- Output: JSON/human dual path (formatter stub) ✔

## Module Status
| Module        | Status                   | Notes                                 |
| ------------- | ------------------------ | ------------------------------------- |
| cli           | implemented stub         | Commands parse & dispatch             |
| config        | stub                     | Loader present; not integrated yet    |
| vm            | stub trait & placeholder | No real Hyper-V logic yet             |
| driver_detect | skeleton                 | Regex + INF walk; needs robust parser |
| package       | stub                     | Version & artifact scan TBD           |
| deploy        | stub                     | Certificate & pnputil logic pending   |
| debug         | stub                     | DebugView orchestration pending       |
| echo_test     | stub                     | Scenario logic pending                |
| output        | stub                     | JSON schema defined                   |
| errors        | stub                     | Needs mapping from domain errors      |
| ps            | implemented wrapper      | Timeout handling to add               |

## Pending Work (High Priority)
1. Robust INF parser (case-insensitive section parse, version extraction).
2. Hyper-V operations (create/snapshot/revert/file copy/execute) via PowerShell commands.
3. Deployment flow (certificate import + pnputil install + PnP verification).
4. Debug capture orchestration & message classification heuristics.
5. Detection tests (metadata override, INF matching, fallback WDM).

## Observability Plan
- Span naming: `driver.detect`, `vm.create`, `vm.snapshot`, `deploy.install`, `deploy.verify`, `debug.capture`.
- Error events include `error.phase` and `error.kind`.
- Timing to be aggregated into `timing_ms` result payload.

## Risks & Mitigations Snapshot
| Risk                            | Mitigation                                |
| ------------------------------- | ----------------------------------------- |
| INF missing                     | Fallback heuristics + user override flag  |
| PS Direct transient failure     | Retry with exponential backoff            |
| Early boot log loss             | Document limitation; optional ETW future  |
| Version mismatch false negative | Multiple enumeration attempts before fail |

## Next Sequence (Implementation Order)
A. INF parser + detection tests
B. PowerShell VM operations wrapper integration
C. Deployment logic (cert + driver install + verify)
D. Debug capture streaming (tail loop + classification)
E. Echo test flows (basic + stress)

## Versioning & Change Control
- Contracts: additive changes only until 0.2.0.
- Breaking trait changes require plan.md update and version bump.

## Tooling
- Testing: assert_cmd in place; add unit tests with cargo test.
- Future: Consider feature flag `parallel` for multi-VM scaling.

