# driver-test-cli v2 — Security and Safety Review

Scope: Security and safety review of the requirements in `requirements.md` for the `driver-test-cli` Rust CLI tool, focused on DebugView distribution, credential handling, certificate trust, privilege usage, command injection, and VM isolation.

---

## 1. DebugView EULA and Redistribution

### Findings
- Requirement L3.1-1 specifies downloading `Dbgview.exe` from `live.sysinternals.com` at runtime if not present, and L3.1-2 requires starting it with `/accepteula`.
- Downloading directly from the official Sysinternals endpoint generally avoids redistribution issues, as the tool is obtained from Microsoft at runtime rather than being bundled with this project.
- However, the requirement does not address pinning to a specific version, verifying integrity, handling offline/air-gapped CI, or what happens if the endpoint changes or enforces different licensing/acceptance flows.

### Risks
- **Licensing/EULA Risk:** Automatically accepting `/accepteula` may violate organizational policy in some environments (e.g., requiring explicit legal review before EULAs are accepted on behalf of the company), especially in CI where runs are non-interactive and may be considered automated mass deployment.
- **Availability Risk:** CI and some developer environments may not have outbound internet access to `live.sysinternals.com`, causing debug capture to fail and potentially breaking the `test` command if debug capture is treated as mandatory.
- **Stability Risk:** The URL, file name, or behavior of the Sysinternals download endpoint can change (including TLS configuration, redirects, or content), causing brittle behavior, unexpected binaries, or silent misbehavior if the tool does not verify the downloaded binary.
- **Integrity Risk:** Without checksum or signature verification tied to an expected DebugView version, there is a risk (however small) of serving malicious or tampered binaries via network compromise or DNS hijack.

### Recommendations
- Treat DebugView as an optional feature: if download fails or is unavailable, degrade gracefully by skipping debug-capture features but still allow driver deployment/verification to proceed, and emit a clear warning plus remediation steps (e.g., manual DebugView deployment or alternate log capture).
- Explicitly document the **EULA acceptance behavior** and require one of:
  - An explicit flag/config (`[observability.debugview] accept_eula = true`) that must be enabled to allow `/accepteula` use in automated environments, or
  - A one-time acceptance flow that records the decision in a per-user config file, with clear messaging that this implies accepting the Sysinternals EULA for that account.
- Provide an optional **"bring your own DebugView"** path: allow users/CI to pre-provision `Dbgview.exe` in a known location in the guest image (or via a host path) and configure the CLI to use it without network download; this lets legal/compliance vet the binary and EULA independently.
- Implement **integrity and origin checks** on the downloaded binary:
  - Pin to HTTPS only; fail hard on certificate/TLS errors.
  - Optionally pin to an expected SHA-256 hash or version string, with a configuration mechanism to update that hash when the team intentionally moves to a new DebugView version.
- If the URL or download fails, log a structured error with:
  - The exact URL used,
  - HTTP status / network error details,
  - A clear recommendation: "Pre-provision Dbgview.exe at `<path>` or disable DebugView-based capture via config." 
- For CI usage, strongly recommend that:
  - Teams either pre-bake DebugView into the VM image or use an internal mirror that has gone through licensing review, and
  - The CLI supports overriding the DebugView download URL via configuration (e.g., `debugview_download_url`), while still enforcing HTTPS and optional checksum verification.

---

## 2. Credential Handling for PowerShell Direct

### Findings
- L1.1-5 requires use of PowerShell Direct (`Invoke-Command -VMName`) as the sole channel for guest command execution.
- In typical developer scenarios, PS Direct can run without explicit credentials if the same user exists on both host and guest with matching credentials and the host user is a Hyper-V administrator.
- In CI, runs are often executed under service accounts (domain or local) that may not map cleanly to guest accounts, and may require explicit credentials (`-Credential`) to access the guest.

### Risks
- **Credential Exposure Risk:** If the CLI accepts usernames/passwords (for the guest) via flags or config, there is a high risk that secrets will be exposed in CLI history, process lists, logs, CI pipelines, or crash reports.
- **Misuse of Service Accounts:** Using the host CI service account directly inside the guest may violate least-privilege; such accounts often have broad rights and should not be reused as generic administrative accounts in test VMs.
- **Auditability and Compliance:** Silent or implicit use of credentials can make it difficult for security teams to audit who has access to which VMs, and from where.

### Recommendations
- Default behavior should **avoid handling raw secrets**: prefer same-user PS Direct (where host and guest share credentials) for local developer workflows whenever possible.
- When alternate guest credentials are needed (common in CI):
  - Do **not** accept passwords on the command line or in plain-text config files.
  - Instead, support referencing secure **credential sources** (e.g., environment variables populated by CI secret stores, Windows Credential Manager entries, Azure Key Vault, or GitHub/AzDO secrets) and convert them into a `PSCredential` in-memory only.
  - Ensure PowerShell invocations never log or echo the credential values, and redact any environment variable values with names matching `*_PASSWORD`, `*_SECRET`, etc. if they appear in error logs.
- Define an explicit configuration model for PS Direct credentials, e.g.:
  - `ps_direct.auth_mode = "same_user" | "named_account"`
  - `ps_direct.credential_ref = "ENV:DRIVER_TEST_VM_PASSWORD"` or `"WIN-CRED:DriverTestVm"`.
- Provide **clear documentation and warnings** around:
  - The privileges that the PS Direct account needs inside the guest (e.g., local admin only, not domain admin).
  - The security implications of using domain-wide service accounts vs per-VM or per-lab accounts.
- For CI, recommend a pattern in which:
  - The PS Direct guest account is a dedicated local admin account on the VM used solely for driver testing.
  - Its credentials are stored and rotated via the CI platform's secret store, never committed to source or plain-text config.

---

## 3. Test Certificate Trust Chain

### Findings
- L2.2-1 requires installing test signing certificates into the guest VM's TrustedPeople **and Root** stores.
- Installing a certificate into the Root store grants it full trust for any certificate chain it issues, not just this project’s driver binaries.
- L2.2-2 and L2.2-3 focus on idempotence and actionable guidance but do not address policy guardrails, scope of trust, or cleanup/removal.

### Risks
- **Over-broad Trust:** Auto-installing a root certificate can unintentionally allow any executable or driver signed with that root to be treated as trusted code on the VM, potentially masking malicious binaries and creating an ideal persistence mechanism for attackers.
- **Policy Violation:** Many organizations have strict controls on root CA trust, even in test environments; silently adding roots from a build tool may violate those controls.
- **Long-Lived Trust:** If certificates are not cleaned up, test roots may persist across VM reuse, snapshots, and image clones, leading to long-lived, hard-to-track trust relationships.

### Recommendations
- Minimize use of the **Root store**:
  - Prefer installing test code-signing certificates into TrustedPeople (or equivalent) where possible and relying on test-signing mode, rather than adding arbitrary roots.
  - Only install into Root if explicitly required for the scenario, and then require a strong, explicit opt-in: e.g., `--allow-root-cert-install` or a config flag with a scary warning.
- Implement **guardrails and transparency**:
  - Before installing any certificate, display/log its subject, issuer, thumbprint, and validity period in structured form.
  - Require a confirmation step in interactive mode (e.g., `--yes` to bypass), and in CI require explicit flags/config to allow non-interactive root installation.
  - Enforce that only certificates located in a specific, user-specified path (e.g., within the repository or a CI-provisioned folder) are installed; do not discover and install arbitrary certs.
- Track installed certificates per VM and per project:
  - Maintain a small registry (file or metadata) inside the VM or as part of the snapshot metadata listing which thumbprints were installed by `driver-test-cli` and when.
  - On `clean` or a dedicated `cleanup-certs` command, remove those certificates from TrustedPeople/Root, with an option to skip cleanup when VMs are used as long-lived test labs.
- For security-conscious environments, document and support a **pre-baked image model**:
  - Teams can build a test VM image with the desired test certificate(s) already installed under controlled conditions.
  - `driver-test-cli` then only validates presence and does **not** install certs itself, avoiding runtime trust modifications.

---

## 4. Privilege Escalation and Admin Requirements

### Findings
- Hyper-V operations (VM creation, snapshot management, state changes, PS Direct) require administrative privileges on the host.
- The requirements specify behavior when Hyper-V or virtualization support is missing (L1.2-9) but do not define behavior when the current process lacks admin rights.
- CI runners may already run with elevated privileges or as service accounts with Hyper-V permissions, but often in **non-interactive** sessions where user prompts (e.g., UAC) cannot be satisfied.

### Risks
- **Unreliable Self-Elevation:** Attempting to self-elevate (UAC prompts) will fail or hang in CI and remote scenarios; it may also confuse users if the tool silently relaunches itself with different privileges.
- **Security Surprises:** Hidden elevation flows or use of alternate accounts can violate the principle of least surprise, and may be disallowed by platform security policies.
- **Partial Privilege:** Running with partial rights (e.g., some Hyper-V operations succeed, others fail) can lead to inconsistent state, half-created VMs, or unclear error reports.

### Recommendations
- **Do not implement automatic self-elevation** inside `driver-test-cli`.
- On startup (or before performing Hyper-V operations), perform a **capability check**:
  - Verify that the current process has required rights (e.g., membership in Hyper-V Administrators, ability to run a trivial Hyper-V cmdlet).
  - If not, fail fast with a clear, actionable error indicating that the tool must be run in an elevated PowerShell or as a user with Hyper-V admin rights.
- Distinguish clearly between:
  - **Interactive developer mode:** Where users can rerun the tool from an elevated shell; error messages should include explicit commands like "Open an elevated PowerShell and run: `driver-test-cli setup`".
  - **CI mode:** Where elevation cannot be granted interactively; error messages should recommend configuration changes to the CI agent (e.g., run agent as a Hyper-V admin service account) rather than suggesting elevation prompts.
- Optionally provide a `doctor` or `check-env` subcommand that validates admin rights, Hyper-V availability, and PS Direct capabilities without performing destructive actions, so teams can bake it into CI setup checks.

---

## 5. Command Injection and Input Handling

### Findings
- The tool will construct PowerShell commands that use user-provided values such as VM names, file paths, and possibly companion application paths.
- Requirements specify structured PS execution and JSON output but do not mandate how inputs are passed to PowerShell (e.g., as parameters vs string concatenation).
- Both the host-side PowerShell and the guest-side scripts are potential surfaces for injection if user input is directly interpolated into command strings.

### Risks
- **Command Injection:** If VM names, file paths, or other inputs are interpolated into PowerShell command strings without proper escaping, an attacker could craft values including characters like `;`, `|`, backtick, or newlines to execute arbitrary commands on the host or inside the guest.
- **Path Confusion:** Maliciously crafted file paths (e.g., using UNC paths or special characters) could cause unintended file access or overwrite locations.

### Recommendations
- Treat all user-supplied values as untrusted and **never** build PowerShell commands by string concatenation.
- Use **parameterized invocations**:
  - On the host: `powershell -Command "& { param($vmName, $path) Copy-VMFile -Name $vmName -SourcePath $path ... }" -ArgumentList @($vmName, $path)` instead of embedding `$vmName` and `$path` directly into the command string.
  - Inside the guest: use `Invoke-Command -VMName $vmName -ScriptBlock { param($destPath) ... } -ArgumentList $destPath` and pass only data parameters.
- Implement **input validation** for key parameters:
  - Restrict VM names to a conservative character set (e.g., alphanumeric, `-`, `_`) and length.
  - Normalize and validate file paths to ensure they are absolute or under specific allowed directories, and reject values containing newlines, control characters, or obvious command separators.
- Avoid dangerous PowerShell features:
  - Do not use `Invoke-Expression` to execute constructed scripts.
  - Avoid `--%` unless strictly necessary, and then only with validated inputs.
- Ensure that any logging of commands or arguments **redacts** sensitive values (paths that may contain secrets, credential references) and never logs raw PS command lines that might reveal injection payloads or secrets.

---

## 6. VM Isolation and Malicious Driver Behavior

### Findings
- The tool orchestrates deployment and execution of arbitrary drivers and companion binaries in a Hyper-V VM, with host/guest integration via PS Direct and `Copy-VMFile`.
- Requirements do not explicitly define the network configuration of the test VM (e.g., isolated switch vs external network) or any constraints on host/guest trust.
- The workflow assumes the VM is a safe sandbox, but driver-under-test code is inherently high-privilege inside the guest and may be malicious or compromised.

### Risks
- **Lateral Movement:** If the test VM has network access to internal systems or the internet, a malicious driver or companion app could use the testing session to move laterally or exfiltrate secrets.
- **Host Escape Surface:** While Hyper-V isolation is strong, PS Direct and Integration Services create additional surfaces (e.g., file copy channel, host-to-guest command channel); any vulnerabilities in Hyper-V or misconfiguration of these channels could be exploited.
- **Trust Bleed:** Reusing the same VM across multiple projects or teams without proper cleanup may allow a compromised VM to influence subsequent tests (e.g., by installing root certificates, persistence mechanisms, or backdoors).

### Recommendations
- Treat the **test VM as untrusted** from the host's perspective:
  - Never execute host commands based on unvalidated data from the guest (e.g., do not interpret guest logs as scripts or config).
  - Keep the integration surface minimal: restrict the tool to PS Direct and `Copy-VMFile`, and avoid adding additional host services or custom channels.
- Recommend and document **isolated networking** for test VMs:
  - Use an internal or private virtual switch for driver test VMs wherever possible, not an external switch connected to production networks.
  - If internet access is required (e.g., Windows Update), consider controlled egress (proxy, firewall rules) rather than full open access.
- Encourage **ephemeral or clean-snapshot usage** patterns:
  - Use baseline snapshots (L1.2-5/6) as a security control: revert to a known-clean state between runs and especially between projects.
  - Optionally support a "paranoid" mode in which the tool always reverts to baseline before and after a test session.
- Clearly **document trust boundaries**:
  - State in the docs that drivers and apps under test are untrusted and may be malicious.
  - Recommend running `driver-test-cli` on dedicated test hosts, not on production or developer primary machines, especially when testing third-party or unvetted drivers.

---

## Overall Assessment

Overall, the requirements describe a powerful test orchestration tool that interacts with privileged host and guest capabilities; with careful implementation and operational practices, it can be reasonably secure.
The main security concerns are around automatic DebugView downloading/EULA acceptance, broad certificate trust via Root store installation, handling of PS Direct credentials, and potential command injection.
Addressing the above recommendations—especially making high-risk behaviors explicit, opt-in, and well-documented—will significantly reduce the attack surface and align the tool with typical enterprise security expectations.
