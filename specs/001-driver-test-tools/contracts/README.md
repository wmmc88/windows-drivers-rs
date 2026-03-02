# Contracts Overview

This directory defines Phase 1 API and interaction contracts for the driver testing CLI toolset.

Contents:
- `cli-contract.md` – CLI surface (commands, flags, exit codes, examples)
- `module-contracts.md` – Internal Rust trait APIs and invariants
- `process-contracts.md` – PowerShell interop command patterns + JSON schemas
- `output-schema.json` – JSON schema for machine-readable results

Change Control:
- Additive only until v0.2.0; breaking changes require spec/plan update and version bump.
