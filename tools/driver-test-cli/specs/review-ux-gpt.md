# UX + Ergonomics Review (GPT): driver-test-cli v2

**Date**: 2026-03-04  
**Inputs**:
- Requirements: `specs/requirements.md`
- Prior UX review: `specs/review-ux.md`

This review validates/challenges the prior reviewer’s findings against the v2 requirements and adds additional UX/ergonomics gaps specific to Windows driver developer workflows.

---

## 1) Validate / Challenge Prior Review Findings

### A. CLI structure (commands, nouns/verbs, layering)

1. **“Keep 5-command structure (test/setup/snapshot/deploy/clean)”** — **CONFIRMED**  
   Matches L4.2-1..5 and aligns with established CLIs (task-oriented verbs).

2. **“`snapshot` might be too granular; consider merging into `setup`”** — **DISAGREE**  
   Snapshot management is a first-class driver-testing workflow (fast reset between iterations) and is explicitly called out as its own command (L4.2-3) and as baseline usage (L1.2-5/6).  
   *Refinement instead of merging*: keep `snapshot` but optimize the 80% case by making `test` default to a baseline revert behavior (when configured) and printing the snapshot in use.

3. **“Add aliases like `snap`”** — **CONFIRMED (optional)**  
   Not required, but low-risk ergonomics. Ensure help text and docs remain canonical on the long form.

4. **“Flags: `--vm-name`, `--json`, `-v/-vv/-vvv`, add `--dry-run`, add `--force` for clean”** — **CONFIRMED with additions**  
   - `--vm-name`, `--json`, `-v/-vv/-vvv` are required (L4.2-6..8).  
   - `clean` must have confirmation (L4.2-5); a `--force`/`-y` non-interactive bypass is important for CI.  
   - `--dry-run` is valuable for preflight (detect/build plan + prerequisite checks) even though not explicitly required.

5. **“Command consolidation similar to cargo/docker/kubectl”** — **CONFIRMED but incomplete**  
   The comparison is directionally right, but the missing “status/logs/doctor” affordances matter more here than aliasing (see Missed Findings).

---

### B. Defaults (behavior with no args, safe defaults)

1. **“No args should show contextual help + auto-detection summary”** — **CONFIRMED**  
   Requirements do not mandate this, but it strongly supports SC-2/SC-8 (fast first success + actionable errors) and reduces “what now?” friction.

2. **“Current directory is implicit; `driver-test-cli .` is unnecessary”** — **CONFIRMED**  
   Also aligns with `cargo` behavior.

3. **“`test` should NOT be the default command”** — **CONFIRMED**  
   A `test` run can be long-running and mutating (VM changes, certificate install, pnputil install); requiring an explicit verb prevents accidental destructive work.

**Additional default nuance (not in prior review):**
- **CONFIRMED NEW**: When `--json` is set, *all machine-readable JSON must go to stdout* and human/progress output should go to stderr (common convention for scriptability).

---

### C. Error messages (format, actionability, Windows specificity)

1. **“Use consistent error format with category + suggested actions”** — **CONFIRMED**  
   This is explicitly required (L4.2-10) and should also map to exit codes (L4.2-9).

2. **Specific templates (Hyper-V missing, VM missing, PS Direct unavailable, cert install failed, version mismatch, no driver package)** — **CONFIRMED, but needs Windows-driver-specific expansions**

   Key gaps vs requirements:
   - **Exit code contract**: Must clearly differentiate user error (exit 1) vs system error (exit 2) (L4.2-9). The human template should state which class it is, and `--json` must encode it.
   - **Transient vs fatal**: The tool MUST classify transient PowerShell errors and retry (L1.1-2/3). UX should surface retry attempts (e.g., “Transient error, retrying 2/3 in 4s…”) and then summarize retries in failure output.
   - **Timeouts**: Must enforce per-operation timeouts (L1.1-4). Errors should explicitly call out which operation timed out and how to increase timeout (via CLI/config).

   Windows-specific error categories the prior review didn’t cover well:
   - **Not elevated / permissions**: Many Hyper-V and VM integration tasks require admin rights or membership in **Hyper-V Administrators**. Error should say “Run PowerShell as Administrator or add user to Hyper-V Administrators; log off/on.”
   - **Copy-VMFile prerequisites**: `Copy-VMFile` depends on Guest Services integration being enabled for the VM and may fail with opaque integration service errors (L1.3-3). Provide direct remediation steps.
   - **DebugView download blocked**: Requirements assume download from `live.sysinternals.com` (L3.1-1). Corporate proxy/TLS interception/offline scenarios need actionable guidance and an offline override path.

---

### D. Progress reporting (TTY vs non-TTY)

1. **“TTY spinners + step breakdown; non-TTY timestamped logs”** — **CONFIRMED**  
   While not explicitly mandated, this is essential for a multi-minute orchestration tool; it also helps meet SC-1/SC-2 by reducing user uncertainty.

2. **“Progress indicators by operation with ETA”** — **CONFIRMED with one caveat**  
   ETA can be unreliable for VM creation and Windows setup; avoid over-promising. Prefer *milestone-based* progress (step N/M) plus elapsed time.

**Missing from prior review (critical):**
- **CONFIRMED NEW**: If a run fails mid-workflow, the tool MUST preserve VM state (L1.2-7) and should *print exactly what it preserved* (VM running/stopped, snapshot state) and provide a one-line “next command” for recovery (e.g., `deploy --force`, `snapshot restore baseline`, `test --skip-build`).

---

### E. JSON output schema (stability, size, artifact pointers)

1. **“Define success and failure JSON schemas”** — **CONFIRMED**  
   Required by L4.2-6 (structured output).

2. **Proposed schema fields (driver/vm/installation/debug_output/companion_app)** — **CONFIRMED but needs tightening**

   Disagreements / fixes:
   - **DISAGREE** with emitting only a single free-form `message` for errors. Requirements imply structured diagnostics (stdout/stderr/exit code separation for PowerShell, L1.1-1; install diagnostics, L2.3-6). JSON should include:
     - `error.kind`: `user` vs `system` (to match exit code 1 vs 2)
     - `error.source`: `powershell` / `pnputil` / `dbgview` / `copy_vmfile` / etc.
     - `error.retry`: attempts made + backoff summary (L1.1-3)
     - `operation`: which step failed (build/deploy/verify/debug/companion)
   - **DISAGREE** with `debug_output.log_file` pointing only to a **guest** path (e.g., `C:\temp\...`) without a host artifact pointer. Since the tool streams logs to the host (L3.1-5), JSON should include both:
     - `guest_log_path`
     - `host_log_path` (where the user should open it)
   - Include `schema_version` at the top level to avoid breaking CI consumers.

3. **Output size controls** — **CONFIRMED NEW**
   `companion_app.output` can be large; JSON should provide truncation + “full output path” rather than dumping unbounded strings.

---

### F. First-run experience

1. **“≤3 commands to first success (check/setup/test)”** — **CONFIRMED**  
   Strongly aligned with the overview goal and SC-2.

2. **“Express setup: `test --setup-if-needed`”** — **CONFIRMED (good power-user mode)**  
   This is very useful in CI/dev loops, but must be safe: only auto-create resources when explicitly requested.

3. **Example flow that downloads a Windows Dev VM image** — **DISAGREE (as written)**  
   Requirements do not specify image acquisition or licensing flows. For Windows driver testing, the VM provenance (VHDX/ISO, version/edition, licensing terms) is a major UX/legal constraint; the CLI should:
   - explicitly state what it will download (if anything), from where, and where it will cache it
   - provide an option to supply a local image path

---

## 2) Missed Findings (Important UX gaps vs requirements and real Windows workflows)

### A. Windows driver dev workflow specifics (EWDK, build tools, elevation)

1. **Build environment detection (EWDK/WDK toolchain)** — **NEW**  
   The requirements assume `cargo build` produces INF/SYS/cat/cert artifacts (L2.1-5), but in practice Windows driver builds often depend on:
   - EWDK/WDK environment variables
   - signing tool availability
   - `cargo make` or repo-specific build scripts

   UX recommendation: on first run (or `test`), run a preflight that detects missing WDK/EWDK prerequisites and prints the exact remediation (e.g., “Open an EWDK Developer Command Prompt” / “Install WDK version X”). Also provide `--build-command <cmd>` / config override to integrate with `cargo make`.

2. **Elevation and “Hyper-V Administrators” checks** — **NEW**  
   Many failures will look like generic PowerShell errors unless the CLI checks privilege early. Provide a clear first-run gate:
   - is process elevated?
   - is user in Hyper-V Administrators?
   - are Hyper-V features enabled?

### B. “When things go wrong mid-workflow” (resumability, partial success)

1. **Stage-aware recovery commands** — **NEW**  
   Driver testing is naturally staged: detect → build → deploy → verify → observe → companion. If build succeeds but deploy fails, the tool should:
   - keep build artifacts
   - point to them
   - offer a minimal rerun path (`deploy` without rebuild, or `test --skip-build`)

2. **VM preservation + explicit debug affordances** — **NEW**  
   Since VM state must be preserved on errors (L1.2-7), the tool should print:
   - VM name
   - VM state (Running/Off)
   - snapshot state (baseline present? reverted?)
   - “How to attach” guidance (`vmconnect.exe` usage, or `Get-VM` / `Start-VM` commands)

### C. Log/artifact management (host locations, retention, discoverability)

1. **Host artifact directory standard** — **NEW (high impact)**  
   Requirements cover log rotation (L3.1-7) but do not specify *where artifacts go*. For UX, define a consistent host-side layout, e.g.:
   - `./.driver-test/` (repo-local) or `%LOCALAPPDATA%\driver-test-cli\` (user-global)
   - per-VM + per-run subfolders containing:
     - JSON result
     - PowerShell transcript (stdout/stderr/exit code per step; required by L1.1-1)
     - DebugView captured log + streamed host copy
     - pnputil outputs / WMI query dumps (when failures occur)

2. **Retention policy and cleanup** — **NEW**  
   Pair “log rotation” with a host retention policy (max runs / max size) and expose cleanup knobs (`clean --artifacts`, or auto-prune).

3. **`--json` should reference artifacts** — **NEW**  
   Machine consumers need stable pointers: include `artifacts_dir` and file names rather than embedding huge logs in JSON.

### D. Comparisons to `cargo test`, `docker compose`, `kubectl` (missing UX patterns)

1. **`status` / `logs` / `doctor` commands** — **NEW**  
   These are the ergonomic equivalents of `docker compose ps`, `docker compose logs`, `kubectl get`, `kubectl logs`, and `cargo` diagnostics. Even if you don’t add commands, at minimum:
   - no-args output should behave like a “doctor summary”
   - provide a way to tail debug logs after a failure without rerunning a full test

2. **Output mode conventions** — **NEW**  
   Many CLIs support `--quiet` / `--verbose`, and `--output json|text`. Here we already require `-v` and `--json`; ensure:
   - default human output is concise (milestones + summary)
   - verbose modes dump per-step PowerShell commands and raw outputs

### E. Networking/offline realities

1. **DbgView download dependency** — **NEW**  
   If `live.sysinternals.com` is unreachable, the tool should fail with:
   - a cached path suggestion
   - an override flag/config (`--dbgview-path`)
   - proxy guidance

---

## 3) Additional New Findings / Recommendations (Actionable)

1. **Config file UX (`driver-test.toml`)** — **NEW**  
   Requirements mandate config load + CLI override (L4.3-1..3). First-run experience should:
   - clearly state which config file was loaded (or “none found”)
   - offer a generated template (even if via a documented copy/paste) with common defaults: VM name, timeouts, artifact dir, snapshot name.

2. **Explicit step labeling in both text + JSON** — **NEW**  
   Every failure should identify the step (`operation`) and the most relevant command output (and where to find full logs). This is the difference between a tool that “automates” vs a tool that users can actually debug.

3. **Make “what will happen” obvious before mutating actions** — **NEW**  
   Particularly for `setup` and `clean`, print a short plan (“Will create VM X with Y GB disk; will download/copy image from Z; will create snapshot baseline”). This reduces fear and aligns with safe CLI norms.

---

## 4) Bottom line

The prior review is broadly aligned with the requirements (commands, `--json`, actionable errors, progress). The most important missing UX pieces are (1) Windows-specific prerequisite/elevation checks, (2) staged failure recovery/resume paths, and (3) explicit host artifact/log management with stable JSON pointers.
