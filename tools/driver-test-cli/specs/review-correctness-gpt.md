# Correctness & Completeness Review: driver-test-cli v2 Requirements

**Reviewer**: GPT-based review  
**Date**: 2026-03-04  
**Scope**: Compare `requirements.md` against the original spec (`001-driver-test-tools/spec.md`), validate/challenge the prior AI review in `review-correctness.md`, and identify any additional issues.

---

## 1. Summary Verdict

- The prior review is **directionally solid**: most of the cited gaps and ambiguities are real, but a few are overstated or misclassified as “critical”.
- I **disagree** that there is *no* build trigger requirement and that echo-sample-specific default patterns are required; these are present or out-of-scope for the v2 requirements.
- I **confirm** the FR nuance losses, most ambiguity findings, and several missing/unclear setup behaviors (especially around VM configuration and test-signing mode), and I add several new findings the prior review did not mention.

---

## 2. Prior Review – "Critical Gaps" (3)

### C1. "No build trigger" for the driver

> Prior finding: No requirement to trigger `cargo build` before deployment; the echo workflow assumes "builds it if needed" but v2 has no FR for it.

- **Assessment: NOT CONFIRMED (but under-specified).**
- `requirements.md` L4.2-1 explicitly defines the `test` command as: "**detect, build, deploy, verify driver**; optionally capture debug output and run companion app.", which is a normative requirement that the tool performs a build step.
- What *is* missing is detail on *when* a rebuild is required (e.g., staleness detection) and with what flags (`--release` vs `--debug`), so this is better treated as an **ambiguity/underspecification**, not a correctness gap.

### C2. "No Windows installation" during first-time VM setup

> Prior finding: First-time setup lacks any requirement for Windows OS installation inside the VM; this is critical to making the test VM actually usable.

- **Assessment: CONFIRMED (as a coverage/clarity gap, not necessarily a requirement to fully automate OS install).**
- Original FR-003 and User Story 1 AS2 require: "creates a new Hyper-V VM **following the setup guidelines from windows-drivers-rs examples folder**, configures it for driver testing" — those guidelines include having a Windows OS installed and configured appropriately.
- v2’s L1.2-1 only requires creating a Generation 2 VM with configurable resources and does **not** state whether the tool assumes a prepared VHD, automates OS installation, or guides the user to install it; that missing contract makes it unclear how FR-003’s intent is fulfilled.

### C3. "No test signing enablement" (e.g., `bcdedit /set testsigning on`)

> Prior finding: The spec does not require enabling test-signing mode in the guest OS, which is necessary for loading test-signed drivers.

- **Assessment: CONFIRMED (as a nuance gap vs FR-007/FR-028).**
- v2 L2.2-1/2/3 cover **certificate installation and guidance**, but there is no requirement that the VM be placed in test-signing mode (e.g., via `bcdedit`) or that this mode be verified.
- Given FR-007/FR-028 and the clarifications about "configures [the VM] for driver testing", the lack of any requirement to check or enforce boot configuration for test-signed drivers is a **real behavioral gap**.

---

## 3. Prior Review – FR Coverage / Nuance Issues (5)

These are the 5 FRs the prior review called out as having "lost nuance" between the original spec and `requirements.md`.

### FR-009: Driver installation tools (devcon vs pnputil vs API)

> Prior finding: Original FR-009 allows devcon, pnputil, or a driver installation API; v2 (L2.3-1) hardcodes `pnputil` only, losing flexibility.

- **Assessment: CONFIRMED.**
- Original FR-009: "load the driver ... using appropriate installation tools (**devcon, pnputil, or driver installation API**)."
- v2 L2.3-1: "MUST install drivers via `pnputil /add-driver <inf> /install` in the guest VM" explicitly narrows the implementation choice and drops the flexibility the original FR envisioned.

### FR-003: Following windows-drivers-rs setup guidelines

> Prior finding: Original FR-003 requires creating a VM "following windows-drivers-rs repository setup guidelines"; v2 L1.2-1 no longer references or enforces these guidelines.

- **Assessment: CONFIRMED.**
- Original FR-003: "create a Hyper-V test VM **following windows-drivers-rs repository setup guidelines**" is more prescriptive about the resulting environment than simply specifying memory/CPU/disk.
- v2 distributes aspects of "configured for driver testing" across multiple L1/L2/L2.2 requirements, but the *explicit* linkage to the repository setup guide (and its implied OS, test-signing, and integration-component configuration) is no longer present.

### FR-014: Console display of debug output

> Prior finding: Original requires real-time console display of captured debug output; v2 only guarantees streaming to host via log tailing.

- **Assessment: CONFIRMED.**
- Original FR-014: "System MUST **display** captured debug output in real-time to the console."
- v2 L3.1-5: "MUST stream captured debug output to the host in near-real-time via log file tailing" never explicitly states that the CLI *prints* that stream to the console, leaving the user-facing behavior under-specified.

### FR-010: Source of the "built version" for comparison

> Prior finding: v2 requirement to match "exact built version" does not clearly state how that version is determined (INF metadata vs binary metadata vs Cargo manifest).

- **Assessment: CONFIRMED (as an ambiguity).**
- Original FR-010: "verify the loaded driver version matches the exact version that was built" makes the *built* artifact the source of truth.
- v2 splits this across L2.1-6 (extract DriverVer from INF) and L2.3-3 (verify loaded driver version matches the exact built version), but does not clearly state whether INF `DriverVer`, cargo metadata, or file version info is normative when they differ.

### FR-031: Hybrid VM lifecycle strategy (reuse vs rebuild)

> Prior finding: Original FR-031 specified a hybrid lifecycle with default reuse, baseline snapshot, and a **force-rebuild flag**; v2 only carries over snapshot creation and revert.

- **Assessment: CONFIRMED.**
- Original FR-031 explicitly requires: persistent VM + baseline snapshot, **default reuse**, and flags to `(a) revert to baseline` and `(b) force a full rebuild`.
- v2 L1.2-5 and L1.2-6 cover baseline creation and manual revert, and L1.2-7 covers preserving state on error, but there is **no requirement for a force-rebuild path or for the default behavior philosophy**, so important lifecycle nuance has been lost.

---

## 4. Prior Review – Ambiguity Findings

### 4.1 "Requirements Too Vague to Implement" (by ID)

For each item from the prior review’s ambiguity table, I judge whether it is indeed too vague for a straightforward implementation.

1. **L2.1-3 – WDM heuristics (`panic = "abort"` + `no_std`)**  
   - **Assessment: CONFIRMED.**  
   - The requirement mentions heuristics but does not define whether *both* indicators are required, what file(s) are inspected (Cargo.toml vs source), or how ties/conflicts with INF metadata are resolved.

2. **L2.1-7 and L2.1-8 – "Adapt detection for repo layout"**  
   - **Assessment: CONFIRMED (high-level but underspecified).**  
   - These requirements state that detection "MUST adapt" to two specific repositories but provide no concrete rules (e.g., expected paths for INF/SYS, workspace vs non-workspace handling), leaving significant design decisions to implementers.

3. **L3.1-6 – Severity classification by content keywords**  
   - **Assessment: CONFIRMED.**  
   - Without either a default keyword mapping, an external configuration mechanism, or a reference to an existing convention, "based on content keywords" is too vague to produce interoperable behavior.

4. **L3.1-7 – Log rotation with configurable maximum message count**  
   - **Assessment: CONFIRMED.**  
   - There is no default, no guidance on what constitutes a "message" (line vs structured record), and no behavior specified when the cap is hit (drop oldest, stop capture, etc.).

5. **L1.1-2 – Classifying PowerShell errors via known patterns**  
   - **Assessment: CONFIRMED.**  
   - "Known error message patterns" are not enumerated nor externalized; without a list or mechanism to extend/override them, different implementations would make incompatible retry decisions.

6. **L1.2-1 – Configurable memory/CPU/disk without defaults or bounds**  
   - **Assessment: CONFIRMED.**  
   - The requirement mandates configurability but omits defaults, minimum viable values, and any guidance on how failures from mis-sizing should be handled.

7. **L4.1-1 – Companion app detection via "conventional directories"**  
   - **Assessment: CONFIRMED.**  
   - "Conventional directories" is undefined; at minimum, `requirements.md` should name the default search locations and patterns (e.g., `bin/`, `target\{debug,release}`, or sibling Cargo `[[bin]]` targets).

8. **L2.3-7 – "Offer to unload existing driver versions"**  
   - **Assessment: CONFIRMED.**  
   - The interaction model is ambiguous: interactive prompt vs `--replace` flag vs always-on behavior; this also has implications for non-interactive CI environments and the "single command" goal.

9. **L1.2-8 – Validate sufficient system resources**  
   - **Assessment: CONFIRMED.**  
   - The requirement does not state which resources (RAM, disk, CPU, etc.) are checked, what thresholds apply, or how those thresholds relate to VM configuration parameters.

### 4.2 "Requirements with Unclear Success Criteria"

These were listed separately by the prior review; I treat them as additional ambiguities.

10. **L3.1-5 – "Near-real-time" streaming of debug output**  
    - **Assessment: CONFIRMED.**  
    - "Near-real-time" is not quantified; without an explicit latency bound (e.g., P95 < 2 seconds), success/failure is subjective.

11. **L2.2-2 – "Already present and trusted" certificate**  
    - **Assessment: CONFIRMED.**  
    - The requirement does not specify how trust is determined (store + EKU + thumbprint? exact cert vs same subject?), which impacts idempotence and security guarantees.

12. **L3.2-1 – "Pattern files or defaults" format**  
    - **Assessment: CONFIRMED.**  
    - There is no description of pattern file format (regex vs glob vs exact match), how patterns are loaded, or how conflicts between defaults and user-provided patterns are resolved.

---

## 5. Other Prior Findings (Architecture, Workflows, Cleanup, Edge Cases)

### 5.1 Layering / Misplaced Requirements

- **L2.1-9 – Architecture compatibility validation**  
  - Prior review: suggested moving from L2 (Driver Operations) to L4 (Orchestration).  
  - **Assessment: PARTIALLY CONFIRMED.** The check conceptually belongs to the orchestration path (pre-flight gating), but tying it to driver detection (L2) is also defensible; it does not create a correctness problem, just minor layering fuzziness.

- **L3.1-1 – Deploying DebugView**  
  - Prior review: argued this is infrastructure (L1) rather than observability (L3).  
  - **Assessment: NOT A REAL ISSUE.** L3 owning the deployment of its own observability tooling is reasonable; reclassifying it into L1 would slightly purify the layering but brings no practical benefit.

### 5.2 Echo Scenario and Companion App Testing

- **Gap: No default validation patterns for echo and other samples**  
  - Prior review: claimed a gap because echo driver behavior lacks built-in default patterns.  
  - **Assessment: NOT CONFIRMED.** Original FR-020 and User Story 3 AS5 talk about validating behavior **"given expected behavior is defined"**; they do not require the tool to ship sample-specific defaults, only to support pattern-based validation when expectations are provided.

### 5.3 Cross-Repo / Workspace / Multi-Driver Edge Cases

- **Cargo workspaces**  
  - Prior review: gap, because workspaces are common and not mentioned.  
  - **Assessment: PARTIALLY CONFIRMED.** The original spec also assumes "current cargo package directory" and does not call out workspace roots; handling workspace-root invocation is a valuable enhancement but not a strict coverage failure relative to the original FRs.

- **Multiple drivers in a single directory / mixed samples in repo**  
  - Prior review: gap, with no rule for disambiguation.  
  - **Assessment: PARTIALLY CONFIRMED.** This is an important real-world edge case, but the original spec likewise scoped behavior to "the current driver package" and did not mandate multi-package discovery; treating this as a design extension rather than a missed requirement seems more accurate.

- **Symlinked directories**  
  - Prior review: gap.  
  - **Assessment: NOT CONFIRMED.** Neither the original spec nor v2 mention symlink handling; unless symlinks are a stated constraint for the target repos, this is an optional robustness improvement, not a correctness issue.

### 5.4 Error Recovery, Cleanup, and DebugView Lifecycle

- **Partial deployment rollback**  
  - Prior review: identified missing cleanup behavior when some steps (e.g., copy) succeed but install fails.  
  - **Assessment: CONFIRMED (as a missing non-functional requirement).** Both specs focus on happy-path and error reporting; neither defines rollback expectations, leaving a gap around repeatability and idempotence.

- **DebugView process cleanup on completion/failure**  
  - Prior review: noted that L3.1-2 starts DebugView but nothing requires stopping it.  
  - **Assessment: CONFIRMED.** Without an explicit requirement to terminate DebugView (and/or redirect logs) after test runs, repeated runs risk accumulating stray processes and stale log captures.

### 5.5 First-Time Setup Details Beyond OS/Test-Signing

- **Enabling/validating integration services / Guest Services for PS Direct and Copy-VMFile**  
  - Prior review: flagged that PowerShell Direct requires appropriate integration components, yet there is no requirement to enable/validate them.  
  - **Assessment: CONFIRMED.** Given L1.1/L1.3’s dependence on PS Direct and Copy-VMFile, a requirement to verify and, where possible, enable needed integration services would make the system more robust and aligns with FR-023/FR-024.

- **"Single command" orchestration semantics**  
  - Prior review: noted that success criteria promise a single command but orchestration details are not fully spelled out.  
  - **Assessment: CONFIRMED (as ambiguity).** L4.2-1 lists what `test` must do but not the exact sequence, gating criteria (e.g., when to bail), or how it interacts with `setup`, `snapshot`, and `deploy` when those commands are also available.

---

## 6. New Findings (Not Called Out in Prior Review)

### 6.1 FR-032 – Channel for File Transfer vs Command Execution

- **Observation:** Original FR-032 (addendum) states that the system "MUST use PowerShell Direct as the sole default channel for in-guest command execution **and file transfer**" on the same Hyper-V host.
- In `requirements.md`, FR-032 is mapped only to **L1.1-5 and L1.1-6** (command execution), while file transfer is handled separately by **L1.3-1 `Copy-VMFile -FileSource Host` (FR-006)**.
- **Finding:** This represents a **semantic shift**: file transfers now rely explicitly on Hyper-V Integration Services rather than PS Direct; the traceability table should either (a) acknowledge FR-032 has been deliberately narrowed/changed, or (b) update L1.3 to reflect the FR-032 intent or revise the original FR.

### 6.2 JSON Output Contract Is Unspecified

- **Observation:** L4.2-6 mandates a `--json` flag for "CI-friendly structured output" but does not define the JSON schema, versioning strategy, or stability guarantees.
- **Finding:** For a CI-facing API surface, lack of a defined schema (even at a high level: top-level fields, status enums, key sections) is a **notable omission** that will affect interoperability and backward compatibility.

### 6.3 Exit Code Semantics Are Incomplete

- **Observation:** L4.2-9 defines three exit codes (0 success, 1 user error, 2 system error) but does not map concrete scenarios to each category.
- **Finding:** Given the breadth of failures (missing Hyper-V, build failures, driver install errors, pattern mismatches, debug capture failures), more precise guidance—or at least examples—of which errors fall into which category would improve correctness and help CI consumers distinguish between transient infra problems and test failures.

### 6.4 Success Criteria Are Sometimes Unrealistically Absolute

- **Observation:** Several success criteria in `requirements.md` are stated as **100%** goals (e.g., SC-3, SC-4, SC-7, SC-8), including "Actionable error coverage | 100% of failure scenarios".
- **Finding:** These are aspirational but not operationally testable as written; relaxing them or clarifying that they are target objectives (e.g., for internal QA) rather than hard acceptance gates would improve the spec’s realism and testability.

### 6.5 VM Naming and Multi-VM Selection Behavior

- **Observation:** L1.2-2 (detect and reuse existing VMs by name) and L4.2-8 (`--vm-name` global override) define how a specific VM is selected, but there is no requirement for behavior when **multiple** VMs match (e.g., naming collisions, old snapshots).
- **Finding:** Given the original spec’s edge case about multiple similar VMs, it would be beneficial to add a requirement clarifying disambiguation strategy (e.g., exact name match only, or fail with an actionable error when multiple candidates are found).

### 6.6 Scope of Automation for OS Provisioning

- **Observation:** While the prior review correctly notes that OS installation is not addressed, neither the original FR-003 nor `requirements.md` clearly state whether OS provisioning is **in scope** for automation vs assumed to have been done manually per the examples.
- **Finding:** Explicitly stating that the tool either (a) expects a pre-configured VHDX and only creates/attaches a VM around it, or (b) will guide or automate OS installation, would avoid divergent assumptions between implementers and users.

---

## 7. Recommendations (High-Level)

Based on both the prior review and the additional findings above:

1. **Tighten FR traceability** where semantics have changed (FR-003, FR-009, FR-031, FR-032) and either accept the changes explicitly or adjust v2 requirements to preserve original intent.
2. **Clarify VM setup responsibilities**: scope of OS provisioning, test-signing, and integration-component configuration; align them with FR-003/FR-031 and Hyper-V/PowerShell Direct dependencies.
3. **Resolve key ambiguities** by: defining at least default heuristics, thresholds, and pattern formats; specifying JSON output schema and exit-code mapping; and disambiguating `test` command orchestration vs other commands.
4. **Add non-functional requirements for cleanup and idempotence**, especially around DebugView lifecycle and partial deployment rollback.
