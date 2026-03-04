# Windows Systems Review (GPT)

**Reviewer**: Windows systems / Hyper‑V / driver tooling SME (GPT)

**Scope**
- Requirements: `driver-test-tool-v2/tools/driver-test-cli/specs/requirements.md`
- Research decisions: `driver-deploy-test-tool/specs/001-driver-test-tools/research.md`
- Previous systems review: `driver-test-tool-v2/tools/driver-test-cli/specs/review-windows-systems.md`

This document validates or challenges the previous reviewer’s findings, calls out additional gaps specific to modern Windows builds (Windows 11 24H2, Windows Server 2025), and adds further recommendations.

---

## 1. Validation of Previous Reviewer’s Findings

This section walks the prior review’s major findings (numbered as in its summary table) and marks each as **CONFIRMED** or explains disagreements/nuance.

### 1.1 Finding #1 – Credentials not addressed for PowerShell Direct

> *Finding:* `Invoke-Command -VMName` requires explicit credentials; design/spec does not address how credentials are created, stored, or supplied.

**Status: CONFIRMED.**
- The spec (L1.1-5, L1.1-6, L4.3 config) never describes credential handling for PowerShell Direct (PS Direct); the research doc similarly assumes `Invoke-Command -VMName` "just works".
- On current Windows (including Windows 11 24H2 / Server 2025), PS Direct **still requires** a valid local or domain account with a password; Microsoft account / AAD-only users / local accounts without passwords will fail.
- The recommendation to provision a dedicated local admin (e.g., during `setup`) and to persist its credential reference in config (or a secure store) is appropriate; the tool should make this explicit in both requirements and UX.

Additional nuance:
- For PS Direct, the host-side default shell is Windows PowerShell 5.1; the credential requirement is unchanged in more recent builds.
- On domain‑joined environments, domain creds work, but this raises additional security and lifecycle questions (password rotation, domain policy); for a test‑automation tool, a VM‑local admin remains the most predictable choice.

### 1.2 Finding #2 – Missing `bcdedit /set testsigning on`

> *Finding:* Installing the test certificate into Root/TrustedPeople is not enough; kernel-mode test‑signed drivers require `bcdedit /set testsigning on` + reboot.

**Status: CONFIRMED.**
- On all supported releases (including Windows 11 24H2 and Server 2025 previews), test‑signed **kernel‑mode** drivers will not load with Secure Boot enabled and `testsigning` off, regardless of certificate store contents.
- The requirement section L2.2 only talks about cert stores; it never mentions testsigning or the necessary reboot; that is a real gap for KMDF/WDM scenarios.
- The prior recommendation to perform `bcdedit /set testsigning on` as part of `setup`, followed by a reboot, and to bake that into the baseline snapshot, is correct.

Nuance for UMDF:
- UMDF drivers are user‑mode binaries loaded by the UMDF framework; they are still subject to code integrity, but testsigning behavior can differ from kernel drivers depending on policy.
- For simplicity, the tool should assume **both UMDF and KMDF/WDM** dev/test drivers require the `testsigning` dev posture and treat the VM as "fully test‑signed".

### 1.3 Finding #3 – Secure Boot conflicts with testsigning and DebugView

> *Finding:* Gen 2 VMs have Secure Boot on by default; Secure Boot and `testsigning` are mutually exclusive in practice; DebugView’s `Dbgv.sys` is test‑signed and also blocked under Secure Boot.

**Status: CONFIRMED (with clarification).**
- On current Windows, Secure Boot enforces that only code signed with keys in the UEFI Secure Boot DB can run at boot and in certain kernel contexts; the `testsigning` BCD option is ignored when Secure Boot is enforced.
- As a result, a Gen 2 VM with Secure Boot left enabled cannot simultaneously (a) load arbitrary test‑signed kernel drivers and (b) rely on DebugView’s test‑signed kernel component.
- Disabling Secure Boot on the test VM is the pragmatic choice for this tool’s target (fast inner‑loop driver iteration), and the spec should call this out as an explicit design decision.

Additional nuance specific to recent builds:
- Windows 11 24H2 and Server 2025 baselines lean harder into virtualization-based security (VBS), HVCI, and the vulnerable driver blocklist; even with Secure Boot off, aggressive security baselines can still prevent some test‑signed or unsigned drivers from loading (see Section 3.3).
- If future advanced scenarios require testing with Secure Boot **on**, that likely belongs in a separate “production‑like” profile that uses attestation‑signed drivers or custom Secure Boot enrollment, not in the default fast‑iteration flow.

### 1.4 Finding #4 – Snapshot revert requires `Start-VM` and readiness wait

> *Finding:* `Restore-VMSnapshot` does not leave the VM in a fully running, PS‑Direct‑ready state; the tool must `Start-VM` (if needed) and wait for guest readiness.

**Status: CONFIRMED.**
- The prior review correctly notes that snapshot (checkpoint) revert is not equivalent to "machine is up and PS Direct ready"; this remains true for current Hyper‑V versions.
- The requirements (L1.2-5/6) talk about creating and restoring snapshots but never describe the follow‑up boot and probe logic; a dedicated `wait_for_guest_ready` primitive is necessary.
- The reviewer’s recommendation to probe with a trivial PS Direct command in a time‑bounded loop is sound; it should be elevated from an implementation detail to a first‑class requirement.

Minor nuance:
- For **production checkpoints** (VSS‑based) vs. **standard checkpoints** (saved‑state‑based), behavior differs slightly, but in both cases you must still ensure the guest advances to a state where the `vmicvmsession` service is running.

### 1.5 Finding #5 – `pnputil /enum-drivers` localization and fragility

> *Finding:* `pnputil /enum-drivers` output is fully localized and not structurally stable enough across locales; structured alternatives via WMI/CIM are preferred.

**Status: CONFIRMED (strongly agree).**
- The current requirement L2.3-2 explicitly calls for parsing `pnputil /enum-drivers` output into metadata; this is locale‑sensitive and will break on non‑English guests.
- As of Windows 11 24H2/Server 2025 previews there is still no documented JSON or machine‑readable output switch for pnputil; the reviewer’s statement that `/format json` does not exist remains accurate.
- `Win32_PnPSignedDriver` (via `Get-CimInstance`) and, where needed, `Get-WindowsDriver -Online`, provide structured, locale‑independent data and are the right foundation for verification.

Nuance:
- `Win32_PnPSignedDriver` only shows drivers associated with device nodes; you still need to capture the published INF name from `pnputil /add-driver` at install time or fall back to `Get-WindowsDriver` for purely staged packages.
- For Windows 11 24H2+, the vulnerable driver blocklist and HVCI can cause drivers to be blocked even though they are staged and appear via WMI; your verification logic should distinguish "staged" vs "started successfully" using device status (e.g., `Get-PnpDevice`).

### 1.6 Finding #6 – DebugView single-instance behavior

> *Finding:* DebugView cannot have multiple concurrent capturing instances; a prior instance will cause the new one to silently fail to capture.

**Status: CONFIRMED.**
- DebugView has long behaved as a single‑client capture tool for kernel and global Win32 streams; another instance (including from a previous test run that crashed) can prevent a new one from getting data.
- The recommendation to terminate stray `Dbgview.exe` processes prior to starting capture is pragmatic; it should be encoded as a precondition in the debug‑capture workflow.

Nuance:
- Because the tool is responsible for managing the test VM, it can be more aggressive than a generic utility: it is reasonable to kill **any** `Dbgview.exe` in the guest before starting a run.
- On highly hardened images, killing arbitrary processes might conflict with policy; consider making this behavior configurable but enabled by default.

### 1.7 Finding #7 – DbgPrint filtering via `Debug Print Filter`

> *Finding:* Modern Windows suppresses most `DbgPrint` output unless `Debug Print Filter` is configured.

**Status: CONFIRMED.**
- The `Debug Print Filter` registry key is the supported way to widen kernel debug output; without it, you may see only a subset of messages, especially on newer builds.
- Baking this into `setup` (and therefore into the baseline snapshot) is necessary if your tests rely on seeing all `DbgPrint`/`KdPrint` output.

Nuance:
- In very recent Windows builds with more aggressive default hardening, even with `Debug Print Filter` opened up, there can still be limitations when other enterprise policies (e.g., some WDAC baselines) are in effect; the tool should document that it expects a non‑WDAC‑locked test image.

### 1.8 Finding #8 – DebugView startup vs driver load race

> *Finding:* There is a real race between starting DebugView and loading the driver; early messages can be lost if you don’t wait for capture to be active.

**Status: CONFIRMED.**
- Waiting on the log file’s creation and/or a small grace period after process start is a minimal, effective way to mitigate this; the spec currently doesn’t encode such a requirement.
- For drivers whose critical diagnostics occur very early (e.g., `DriverEntry` or early UMDF initialization), this race can meaningfully affect test reliability; the tool should treat "capture active" as a prerequisite before triggering install/start.

Nuance:
- You might also want a **post‑install dwell time** where DebugView stays active for some seconds even after the test concludes, to catch late‑arriving messages (e.g., asynchronous worker threads or delayed cleanup).

### 1.9 Finding #9 – Guest Service Interface dependence for `Copy-VMFile`

> *Finding:* `Copy-VMFile` depends on the Guest Service Interface integration service, which is often disabled by default; using `Copy-Item -ToSession` via PS Direct avoids this.

**Status: CONFIRMED (and I agree with the alternative recommendation).**
- The requirements mandate `Copy-VMFile` (L1.3-1) without mentioning the need to enable `"Guest Service Interface"`; that’s an operational footgun.
- Given the design already depends on PS Direct and credentials, using `Copy-Item -ToSession` as the **primary** path and `Copy-VMFile` as a secondary option (or not at all) simplifies setup.

Nuance:
- On Windows 11+ with hardened Hyper‑V defaults, integration services tend to be enabled, but relying on that default is still risky; controlling the entire stack via PS Direct is more self‑contained.
- `/ToSession` copies are slower for large payloads; however, driver packages and test tools are small enough that this trade‑off is acceptable.

### 1.10 Finding #10 – No severity or PID/source in DebugView logs

> *Finding:* DebugView’s log file format contains only sequence, elapsed time, and message; severity and kernel vs user source cannot be inferred from structure.

**Status: CONFIRMED.**
- The spec’s idea of classifying messages by severity (L3.1-6) must therefore be based on agreed‑upon textual conventions in driver log messages, not on any inherent marker.
- Recommending a structured prefix scheme (e.g., `[ERR]`, `[WARN]`, `[INFO]`) for test drivers is a good mitigation and should be captured in documentation and perhaps template code.

Nuance:
- For production‑oriented drivers, using ETW/TraceLogging is still the better long‑term path; the current DebugView‑based approach is reasonable for early development but should be presented as such.

### 1.11 Finding #11 – Kernel vs user origin indistinguishable in DebugView logs

> *Finding:* You cannot tell from the log line alone whether a message came from kernel‑ or user‑mode.

**Status: CONFIRMED.**
- This does limit some of the ambitions in L3.1-3 vs L3.1-4 (separate guarantees for KMDF/WDM vs UMDF logs); the tool can only say "we saw this text".
- Where it matters which side emitted a message, the driver/test code should encode that in the text (e.g., `[KMD]` vs `[UMD]`).

Nuance:
- If you later move to ETW, you can distinguish providers and event sources much more precisely; that could be a v2+ enhancement.

---

## 2. Critical Gaps and pnputil vs Get‑CimInstance – Overall Position

### 2.1 Three Critical Gaps (Credentials, Testsigning, Secure Boot)

I **fully agree** with the previous reviewer that these three are true critical gaps:

1. **Credentials** – Without a defined credential provisioning and storage model, PS Direct is not usable in a repeatable way; the tool would either fail unpredictably or prompt interactively, which breaks CLI/CI scenarios.
2. **Testsigning** – Merely installing certificates is insufficient; without `bcdedit /set testsigning on` + reboot (captured in the baseline), kernel‑mode test‑signed drivers will silently fail to load, undermining the tool’s core purpose.
3. **Secure Boot** – Leaving Secure Boot enabled while expecting testsigning to work is self‑contradictory; the default fast‑iteration path should **explicitly** disable Secure Boot on the test VM.

These need to be elevated from review notes into explicit requirements and/or design decisions in `requirements.md`.

### 2.2 pnputil Text Parsing vs Get‑CimInstance / WMI

I also **agree** with the recommendation to avoid `pnputil /enum-drivers` text parsing as the primary verification mechanism and to rely instead on WMI/CIM:

- WMI (`Win32_PnPSignedDriver`) and `Get-PnpDevice` are structured, locale‑independent, and easier to evolve as Windows adds new fields.
- `pnputil` remains appropriate for actual install/uninstall operations, where exit codes and simple success/failure text are enough.
- Combining `pnputil` for installation with CIM queries for verification gives you robust driver state and metadata without tying you to localized output.

I would strengthen the recommendation: **treat `pnputil /enum-drivers` parsing as a last‑resort diagnostic tool only**, not as a core requirement, and update L2.3-2 to center on CIM rather than pnputil.

---

## 3. Additional Issues the Previous Review Missed

This section focuses on areas explicitly requested by the user where the previous review did not go deep enough or at all.

### 3.1 Windows 11 24H2 and Server 2025 Security Baselines

Recent Windows releases continue to increase default security posture, especially around kernel integrity and virtualization‑based security (VBS).

Key implications for this tool:

1. **VBS / HVCI default posture**
   - On clean‑installed Windows 11 (including 24H2), VBS and memory integrity (HVCI) are frequently on by default on capable hardware, particularly for consumer SKUs; Server 2025 baselines also encourage VBS/HVCI.
   - HVCI enforces additional constraints on kernel drivers (e.g., no unsupported control‑flow constructs, stricter signing and mitigation requirements).
   - Many test‑signed or experimental drivers that load fine with VBS/HVCI off will fail to load with memory integrity on, even if testsigning is enabled.

   **Recommendation:**
   - The `setup` flow should **explicitly disable VBS/HVCI** in the guest for the default fast‑iteration scenario (e.g., ensure "Memory integrity" is off, or apply the documented registry/group policy equivalent), and this state should be captured in the baseline snapshot.
   - Document that testing against production‑like HVCI baselines is a separate (advanced) scenario; attempting to support both in one "magic" setup will create confusing, flaky behavior.

2. **Vulnerable driver blocklist**
   - Modern Windows includes a curated vulnerable driver blocklist that is enforced under certain security configurations (e.g., HVCI, certain WDAC/SAC policies).
   - If your test driver matches a blocked pattern (unlikely but possible in some experiments), it may be prevented from loading even though staging and signing succeed.

   **Recommendation:**
   - When verifying device state (L2.3-5), explicitly surface when installation succeeded but the device/problem code indicates code integrity or blocklist rejection.
   - Provide guidance in error messages that the test image is expected to be free of WDAC/SAC enterprise policies and that HVCI is disabled for dev scenarios.

3. **TPM / encryption / BitLocker considerations**
   - 24H2/Server 2025 images may default to BitLocker or device encryption when a virtual TPM is present.
   - For a throwaway test VM, automatic disk encryption adds little value and can complicate snapshot and restore performance.

   **Recommendation:**
   - For dev/test VMs, **omit the vTPM** by default; if users explicitly request a more production‑like VM, that can be a configurable option.

### 3.2 Smart App Control (SAC) and Test-Signed Drivers / Tools

Smart App Control (introduced in Windows 11 22H2 and evolving in later releases) is primarily about **user‑mode** code, but it still affects this tool’s ecosystem.

Key points:

1. **Scope of SAC**
   - SAC enforces WDAC‑style policies on unknown/untrusted user‑mode applications, especially in "Evaluate" and "On" modes.
   - It does **not directly govern kernel driver loading** in the same way Secure Boot + KMCI/HVCI do, but it can block your **companion applications**, test harnesses, and **DebugView** itself if they appear as untrusted binaries.

2. **Host vs guest SAC**
   - The CLI tool runs on the **host**; if host SAC is in "On" mode, locally built unsigned Rust binaries and downloaded utilities (DebugView, WDK tools) may be blocked or require explicit user consent.
   - Inside the **guest**, SAC may be present on consumer SKUs; again, it could block your companion test app and potentially DebugView.

   **Recommendation:**
   - The spec should acknowledge that the tool assumes both host and guest are **not** under SAC "On" mode for the default path; if SAC is enabled, the user must either sign the helper binaries or configure exceptions.
   - For CI scenarios, recommend using enterprise‑oriented test images where WDAC/SAC are disabled or under explicit control, rather than fresh consumer 24H2 installs with SAC auto‑enabled.

3. **Interaction with test-signed components**
   - SAC’s decisions are based largely on reputation and signing; test-signed user‑mode binaries may fare better than completely unsigned ones but are not guaranteed to be allowed.
   - For kernel drivers, testsigning and Secure Boot/HVCI remain the dominant gates; SAC influences the supporting tools around them.

   **Practical upshot:**
   - The most common SAC failure mode for this tool will be "DebugView or the test companion app won’t launch" rather than "driver won’t load".
   - Error messaging should call out SAC/WDAC as a potential cause when process start fails with access denied / blocked‑by‑policy style errors.

### 3.3 Memory Integrity (HVCI) in Detail

The previous review mentioned test‑signing and Secure Boot but did not dive into HVCI.

Implications for the driver test tool:

1. **Driver compatibility**
   - HVCI requires drivers to be compatible with certain mitigations (e.g., no unsupported kernel stack manipulations, no legacy patchguard‑hostile behavior); early‑stage or experimental drivers may intentionally not meet these.
   - Even when test‑signed and otherwise correct, drivers that violate HVCI rules will fail at load or soon after, sometimes with less obvious error codes.

2. **Environment consistency**
   - If some test VMs created by `setup` have HVCI on and others off (for example, due to differing base images), behavior will be highly inconsistent.

   **Recommendation:**
   - Make HVCI state an **explicit configuration parameter** for test VMs, with the default being **off** for fast dev iteration.
   - When HVCI is on, surface a clear warning in the CLI output so that failures are interpreted in that context.

### 3.4 PowerShell 5.1 vs 7.x for PowerShell Direct

The previous review correctly emphasized PS Direct semantics but did not explicitly distinguish between Windows PowerShell 5.1 and PowerShell 7.x (`pwsh`).

Key points:

1. **PS Direct is a Windows PowerShell feature**
   - The `-VMName` parameter on `Invoke-Command` and `Enter-PSSession` is implemented by Windows PowerShell remoting (5.1) and the Hyper‑V integration stack.
   - PowerShell 7+ can interoperate with 5.1 remoting, but the simplest and most reliable path is to **explicitly invoke `powershell.exe` 5.1** for all host‑side PS Direct operations.

2. **Tool behavior on hosts where `pwsh` is default**
   - On some developer machines, `pwsh` may be the default shell and/or in front of `powershell.exe` in PATH.
   - The research doc already uses `powershell.exe` explicitly; that is good and should be mandated in the requirements to avoid accidental reliance on PS 7 behavior.

3. **Guest shell version**
   - PS Direct targets the guest’s **Windows PowerShell** by default; whether the guest has PowerShell 7 installed is irrelevant unless your scripts explicitly invoke `pwsh`.

   **Recommendation:**
   - Clarify in L1.1 that the tool relies on **Windows PowerShell 5.1** on the host for PS Direct operations and will explicitly call `powershell.exe`.
   - If a future version wants to support `pwsh`, it should do so via explicit compatibility shims, not by changing the base assumption.

### 3.5 WinRM as a Fallback When PS Direct Fails

The spec and previous review position PS Direct as the sole guest command channel.

Real‑world considerations:

1. **Cases where PS Direct is unavailable**
   - VM not hosted on the local Hyper‑V instance (e.g., remote Hyper‑V server, cluster scenarios).
   - Non‑Windows guests (outside current scope, but future‑proofing).
   - Misconfigured or disabled `vmicvmsession` integration service.

2. **WinRM/WSMan as a network‑based fallback**
   - WinRM remoting (`Invoke-Command -ComputerName` or `-HostName`) can work over the network, independent of PS Direct and the Hyper‑V integration channel.
   - It requires network configuration (IP, firewall, listeners, auth), which undermines some of the elegance of the PS Direct design.

   **Recommendation:**
   - For **v2 of this tool**, keeping PS Direct as the only supported transport is reasonable and aligns with the requirements; WinRM adds complexity that may not be worth it yet.
   - However, it would be wise to design the Rust abstraction layer so that the command‑execution backend is pluggable (PS Direct now, WinRM later), to avoid painting yourself into a corner.
   - The error messages when PS Direct is unavailable should clearly say "this tool currently requires PS Direct" so users understand that network‑based remoting is not (yet) supported.

### 3.6 Other Gaps and Clarifications

Beyond the explicitly requested topics, a few additional items deserve mention:

1. **Checkpoint type and consistency**
   - Hyper‑V distinguishes between "standard" and "production" checkpoints; the latter uses VSS and may behave differently for services and drivers that are sensitive to being snapshotted.
   - For a driver test environment, standard checkpoints are usually adequate and simpler; the spec should state which is expected and why.

2. **Handling automatic reboots and BSODs**
   - Aggressive Windows Update or crash‑reboot settings can cause the guest to reboot during or after driver install, confusing the tool.
   - The prior review briefly mentions disabling auto‑restart after BSOD; this should be formalized in `setup`.

3. **Time synchronization**
   - DebugView timestamps are relative; correlating host logs, guest debug output, and CLI events is easier if host and guest clocks are reasonably in sync.
   - Hyper‑V integration time sync is usually sufficient, but if disabled, your correlation may be off.

   **Recommendation:**
   - Ensure the "Time synchronization" integration service remains enabled in the test VM configuration.

4. **Host/guest OS skew**
   - Running a very new guest (e.g., 24H2) on an older host Hyper‑V (e.g., Server 2019) can lead to subtle integration differences.
   - While generally supported, it’s wise to document a **tested matrix** of host/guest versions for which the tool is validated.

---

## 4. New Findings and Recommendations

This section summarizes additional concrete recommendations beyond the prior review.

### 4.1 Make Security Posture an Explicit Profile

Right now, expectations around Secure Boot, testsigning, HVCI, SAC, and WDAC are implicit and scattered.

**Recommendation:**
- Introduce an explicit **"dev-fast" security profile** in the requirements, encompassing:
  - Secure Boot **off**.
  - Testsigning **on**.
  - HVCI/memory integrity **off**.
  - No WDAC/SAC enforcement in the guest; host SAC off or configured to trust helper tools.
  - vTPM **absent** by default.
- Optionally define a future **"prod-like" profile** where Secure Boot and HVCI are on and drivers are attestation‑signed, but make it clear this is out of scope for the initial CLI.

### 4.2 Strengthen Error Classification Around Modern Security Failures

Given the additional enforcement points in recent Windows releases, generic "driver failed to start" is insufficient.

**Recommendation:**
- When a driver installation appears to succeed but the device status is not OK, explicitly inspect:
  - Problem codes (e.g., via `DEVPKEY_Device_ProblemCode`).
  - Relevant event logs (e.g., System / Microsoft‑Windows‑Kernel‑Pnp / CodeIntegrity events) for common patterns indicating code integrity rejection or blocklist hits.
- Map those to distinct error categories: `CodeIntegrityBlocked`, `HVCIIncompatible`, `BlockedByPolicy`, etc., and surface targeted remediation tips (e.g., "Disable memory integrity in this test VM" or "Use a non‑WDAC‑locked image").

### 4.3 Clarify Host Toolchain Requirements

The research assumes the presence of certain host tools (Hyper‑V cmdlets, Windows PowerShell, etc.) but doesn’t spell them out as hard requirements.

**Recommendation:**
- In L1.x, explicitly require:
  - Windows host SKU that supports Hyper‑V (no Home editions without optional Hyper‑V, no Nano Server, etc.).
  - Hyper‑V role enabled and `vmms` running.
  - Windows PowerShell 5.1 available and on PATH.
- Consider a preflight `doctor`/`check` command that validates the environment before attempting `setup`.

### 4.4 Prepare for Multi-VM / Parallelization

While the current spec focuses on a single VM, future test scenarios may want to run multiple VMs in parallel.

**Recommendation:**
- Design the PS interop layer and VM abstraction so that VM name and credential are always explicit parameters (no globals), enabling parallel invocations later.
- Avoid design choices that implicitly tie you to a singleton VM per host process.

---

## 5. Conclusion

- The previous Windows systems review is broadly accurate; its key findings – particularly around credentials, testsigning, Secure Boot, and avoiding pnputil text parsing – are **confirmed** and should be promoted into the formal requirements and design.
- Modern Windows (11 24H2, Server 2025) security posture introduces additional friction points (HVCI, SAC/WDAC, vulnerable driver blocklist, vTPM/BitLocker) that must be handled explicitly, at least by defining a clear "dev‑fast" security profile and improving error classification.
- With these adjustments, the overall architecture (PS Direct + Rust CLI + Hyper‑V VMs + DebugView‑based observability) remains sound for its stated goal of fast, automated driver testing in isolated VMs.
