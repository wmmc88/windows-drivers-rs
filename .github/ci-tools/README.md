# CI tool cache

CI installs LLVM and the WDK from a GitHub Actions cache instead of calling WinGet, the PowerShell Gallery or nuget.org on every job. Those calls were the main source of flaky setup failures (PSGallery 403s, WinGet rate limits, transient download errors) and about a third of runner time.

## How it works

The cache holds **inputs to the installers, not installed state**: the pinned LLVM installer, an offline layout of the Windows SDK and WDK (for 10.0.22621), or the set of `.nupkg` files for the NuGet WDK (10.0.26100). Every job, cached or not, runs the same install code against those local files. Because the cache never carries registry, PATH or filesystem side effects, a restored run cannot behave differently from a fresh one.

Everything pinned lives in [`manifest.json`](./manifest.json), including the SHA-256 of each installer, bootstrapper, layout tree and package. A payload is only used if it matches those digests, both when it is saved and when it is restored.

| Piece | Role |
|---|---|
| [`setup-toolchain`](../actions/setup-toolchain/action.yaml) | Used by `test`, `build`, `docs`, `lint`, `local-development-makefile` and `codeql`. Restores the exact cache key for the requested versions, verifies it, installs offline. Never touches the network itself. |
| [`cache-update.yaml`](../workflows/cache-update.yaml) | The only workflow that talks to WinGet, PSGallery or nuget.org, and the only one that saves cache entries. |
| [`cache-guard.yaml`](../workflows/cache-guard.yaml) | Runs the Pester tests, and on pull requests and merge groups warns early when a pinned payload is not cached yet. |
| `CiTools.psm1` | Manifest schema, payload keys, verification and the offline installers. |
| `CiToolsAcquire.psm1` | Network-facing acquisition used by Cache Update and `Update-Manifest.ps1`. |
| `CiToolsGuard.psm1` | Firewall guard used by the verification jobs. |

## When the cache has no entry

Consumer jobs fail instead of downloading. That keeps results deterministic and keeps flaky endpoints out of the daily runs. The failure says which payload is missing. Cache Update is dispatched on the default branch automatically when a pull request pins something new (Cache Guard does this for same-repository pull requests and merge groups), and you re-run the failed jobs once it succeeds. Maintainers can also start it by hand from the Actions tab.

## Updating pins

Run `Update-Manifest.ps1` from a Windows machine that has `winget` and `nuget`, then open a pull request with the manifest diff. The digests it records are trusted on first use, so review them like any other dependency change.

```powershell
.\.github\ci-tools\Update-Manifest.ps1 -AddLlvm 19.1.7
.\.github\ci-tools\Update-Manifest.ps1 -RefreshNuget 10.0.26100
.\.github\ci-tools\Update-Manifest.ps1 -Format   # canonical formatting, no network
```

Adding an LLVM pin to the manifest and to the `matrix` of a workflow is all a new version needs. Cache Update then seeds both architectures, and the verification matrix grows to include it.

To pre-warm the cache for an unmerged pull request, run Cache Update on the default branch with `manifest-ref` set to `refs/pull/<number>/head`. Only the manifest data is taken from that ref, never code.

## Forcing a fresh copy

If a newer patch release fixes something CI needs, do not wait for the weekly run: run Cache Update manually with **ignore-cache** enabled. It rebuilds every payload from its source and replaces the cached copy only after the new one has verified. Changing a pin in the manifest also changes its cache key, which is the normal way to move to a new version. Bumping `generation` invalidates everything.

## Weekly run

Cache Update runs every Sunday off-peak, away from the daily 11:00 UTC CI runs, and does four things:

1. **Cold canary.** It rebuilds every payload from its source with the cache ignored, so an installer that disappeared upstream is noticed before the cached copy is evicted. Payloads are acquired one at a time to keep calls to any one endpoint serial.
2. **Verification.** Every runner, LLVM, WDK and target combination installs from the cache through the same `setup-toolchain` action, with outbound traffic blocked for everything except the runner and its bundled Node.js. A hidden download makes the job fail. For NuGet WDKs the installed tree is also hashed against the manifest. The blocked-traffic report in the job summary is informational for now.
3. **Update check.** It looks for newer NuGet packages for each pinned WDK and for newer LLVM releases. A NuGet update opens or refreshes a pull request from `ci/tool-cache-bump`. If a version that is already pinned resolves to different content, the run fails instead of proposing a change.
4. **Notice.** A newer LLVM release is only reported. Adding it to the matrix stays a deliberate change.

The pull request is opened with the workflow token, so it needs to be closed and reopened to start CI. The same checks can be run on demand with the `check-updates` input.

## Repository settings

Entries are evicted after seven days without a restore, which the daily CI runs and the weekly verification prevent. Current pins total roughly 3 GB, and each additional LLVM version adds about 0.7 GB for both architectures. A 30 day retention and a 50 GB limit leave room for the planned LLVM matrix. Opening the weekly NuGet update pull request needs **Allow GitHub Actions to create and approve pull requests** turned on under Settings, Actions, General. Without it the update branch is still pushed and the run summary links to the compare page.

## What is not covered

* Cargo registry and `target` caching is deliberately out of scope.
* The preinstalled Windows kit uninstall for 22621 still runs on every job, because the runner images ship a conflicting kit. It uses the Add/Remove Programs entries directly and needs no network.
* A pull request from a fork does not get a token that can start Cache Update, so it gets the advice only.

## Tests

```powershell
Invoke-Pester -Path .github/ci-tools
```

This needs Pester 5, which the Windows runner images include. The tests cover manifest validation, key derivation, digest and tree verification, install planning and the verification matrix. Network and installer paths are exercised by Cache Update itself.
