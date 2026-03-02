# CLI Contract

Version: 0.1.0
Status: Draft (Phase 1)

## Global Flags
| Flag               | Type   | Description                                | Notes                        |
| ------------------ | ------ | ------------------------------------------ | ---------------------------- |
| `--json`           | bool   | Emit JSON output                           | Switches formatter layer     |
| `-v/--verbose`     | count  | Increase verbosity (WARN→INFO→DEBUG→TRACE) | Up to 3 times                |
| `--vm-name <NAME>` | string | Override default VM name                   | Default resolves from config |
| `--help`           | bool   | Print help                                 | Provided by clap             |
| `--version`        | bool   | Print version                              | Provided by clap             |

## Commands
### `driver-test` (implicit `test` subcommand alias future TBD)
Detect, build, deploy, and verify a driver.

Flags:
| Flag                    | Type | Description                           |
| ----------------------- | ---- | ------------------------------------- |
| `--package-path <PATH>` | path | Root of driver package (default: cwd) |
| `--revert-snapshot`     | bool | Revert VM to baseline before run      |
| `--rebuild-vm`          | bool | Force VM recreation before test       |
| `--capture-output`      | bool | Enable debug output capture           |
| `--driver-type <KMDF    | UMDF | WDM>`                                 | string | Manual override for detection |

Exit Codes:
| Code | Meaning                  | Examples                                       |
| ---- | ------------------------ | ---------------------------------------------- |
| 0    | Success                  | All verification checks pass                   |
| 1    | User error               | Invalid path, unsupported driver type override |
| 2    | System failure           | Hyper-V interaction error, PS execution error  |
| 3    | Partial success (future) | Driver loaded but validation incomplete        |

JSON Output (success):
```json
{
  "status": "ok",
  "driver": {
    "name": "example",
    "type": "KMDF",
    "version": "1.2.3",
    "architecture": "x64"
  },
  "vm": {
    "name": "wdk-test-vm",
    "snapshot": "baseline-driver-env"
  },
  "pnp": {
    "device_count": 1,
    "matching_ids": ["ROOT\\EXAMPLE"],
    "loaded_version_ok": true
  },
  "debug": {
    "captured": true,
    "message_count": 42,
    "errors": 0
  },
  "timing_ms": {
    "build": 123456,
    "deploy": 3456,
    "verify": 789
  }
}
```

JSON Output (error):
```json
{
  "status": "error",
  "code": 2,
  "phase": "deploy",
  "message": "Hyper-V VM not reachable via PowerShell Direct",
  "details": {
    "vm": "wdk-test-vm",
    "stderr": "A remote session might have ended"
  }
}
```

### `driver-test setup`
Create baseline VM.

Flags:
| Flag               | Type   | Description                 |
| ------------------ | ------ | --------------------------- |
| `--vm-name <NAME>` | string | VM name (required)          |
| `--memory <MB>`    | int    | Memory in MB (default 2048) |
| `--cpu-count <N>`  | int    | CPUs (default 2)            |
| `--disk-size <GB>` | int    | Disk size GB (default 60)   |

Outputs summary table (human) or JSON:
```json
{ "status": "ok", "vm": {"name": "wdk-test-vm", "memory_mb": 2048, "cpu_count": 2, "disk_gb": 60, "snapshot": "baseline-driver-env"} }
```

### `driver-test snapshot`
Manage baseline snapshot.

Flags:
| Flag               | Type   | Description                  |
| ------------------ | ------ | ---------------------------- |
| `--create`         | bool   | Create new baseline snapshot |
| `--revert`         | bool   | Revert to existing baseline  |
| `--vm-name <NAME>` | string | Override VM name             |

### `driver-test clean`
Remove VM.

Flags:
| Flag               | Type   | Description              |
| ------------------ | ------ | ------------------------ |
| `--yes`            | bool   | Skip confirmation prompt |
| `--vm-name <NAME>` | string | VM to remove             |

Exit Codes: 0 success, 1 not found, 2 Hyper-V failure.

## Error Message Guidelines
User errors MUST provide actionable remediation line, e.g.:
```
ERROR: Driver package path not found: C:\bad\path
ACTION: Verify path or run `dir` to inspect available directories.
```
System failures MUST include phase, e.g. `PHASE=vm.create`.

## Versioning
- Add new commands: minor version bump
- Change exit codes: major version bump
- Add fields to JSON responses: backward compatible (allowed)

## Compatibility Guarantees (0.x window)
- Field removals not allowed
- Additional optional flags allowed
- Output ordering not guaranteed for human mode; JSON stable keys required

