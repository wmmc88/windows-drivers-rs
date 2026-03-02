# Process & PowerShell Interop Contracts

Version: 0.1.0
Status: Draft

## Wrapper Behavior
Scripts wrapped as:
```
$ErrorActionPreference='Stop'; try { <SCRIPT> | ConvertTo-Json -Compress } catch { $_ | ConvertTo-Json -Compress | Write-Error; exit 2 }
```
- Success: stdout = JSON value, exit code 0
- Failure: stderr includes JSON of error record, exit code 2

## Error JSON Shape (PowerShell)
```json
{
  "ExceptionType": "System.Management.Automation.RuntimeException",
  "Message": "A remote session might have ended",
  "CategoryInfo": "OperationStopped",
  "FullyQualifiedErrorId": "SomeId"
}
```

Mapping:
| PS Field              | Internal            | Notes                 |
| --------------------- | ------------------- | --------------------- |
| Message               | VmError::Ps(String) | Raw message preserved |
| ExceptionType         | error.detail        | For classification    |
| FullyQualifiedErrorId | error.code          | Optional              |

## Command Patterns
### Create VM
```
New-VM -Name $name -MemoryStartupBytes ${memory_mb}MB -Generation 2
```
Follow-ups:
- `Set-VMProcessor -VMName $name -Count $cpu_count`
- Optional disk sizing if not auto-done

### Create Snapshot
```
Checkpoint-VM -Name $name -SnapshotName $baseline
```

### Revert Snapshot
```
Restore-VMSnapshot -VMName $name -Name $baseline -Confirm:$false
```
(Requires VM off; if running, stop first)

### File Copy Host→Guest
```
Copy-VMFile -Name $name -SourcePath $src -DestinationPath $dest -FileSource Host -CreateFullPath
```
Retry if integration services not ready (up to 5 attempts, 2s * 2^n backoff).

### PowerShell Direct Command
```
Invoke-Command -VMName $name -ScriptBlock { <guest ops> }
```
Credentialless if same host user context; otherwise add `-Credential (Get-Credential)`.

### Driver Install (Guest)
```
pnputil /add-driver C:\Drivers\MyDriver\my.inf /install
```
Verify:
```
pnputil /enum-drivers | findstr /i mydriver.sys
```

### Certificate Install (Guest)
```
Import-Certificate -FilePath C:\Certs\TestCert.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
Import-Certificate -FilePath C:\Certs\TestCert.cer -CertStoreLocation Cert:\LocalMachine\Root
```

### DebugView Launch (Guest)
```
& C:\Tools\Dbgview.exe /k /t /q /accepteula /l C:\DriverLogs\dbwin.log
```

### Tail Debug Log
```
Get-Content C:\DriverLogs\dbwin.log -Wait -Tail 0
```

## Transient Error Classification
| Pattern                              | Class         | Retry? | Backoff               |
| ------------------------------------ | ------------- | ------ | --------------------- |
| "A remote session might have ended"  | GuestNotReady | Yes    | exponential (cap 30s) |
| "The system cannot find the file"    | FileNotFound  | No     | -                     |
| "Access is denied"                   | Permission    | No     | -                     |
| "PowerShell Direct is not supported" | Unsupported   | No     | -                     |

## Timeouts
Default per operation:
| Operation         | Timeout      |
| ----------------- | ------------ |
| VM create         | 15m          |
| Snapshot create   | 2m           |
| File copy         | 2m per 100MB |
| PS direct execute | 60s          |
| Driver install    | 120s         |
| DebugView start   | 30s          |

Exceeding timeout → kill process and return PsError::Timeout.

## Structured Logging Fields
Every process invocation attaches:
- `ps.command` (truncated to 200 chars)
- `ps.duration_ms`
- `ps.retry_count`
- `ps.success` (bool)

## Future Extensions
- ETW session for kernel events
- Parallel file transfers (requires rate limiting)
