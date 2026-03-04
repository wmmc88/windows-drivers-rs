//! CLI command definitions and orchestration.

use crate::config::{Config, DEFAULT_CONFIG_FILE};
use crate::debug::{self, CaptureConfig};
use crate::deploy::{self, PnpDeployer};
use crate::detect;
use crate::echo::{self, CompanionApp};
use crate::errors::AppError;
use crate::output::{self, Progress, TestResult};
use crate::vm::{HypervProvider, VmProvider};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "driver-test-cli",
    version,
    about = "Automate Windows driver deployment and testing in Hyper-V VMs."
)]
pub struct Cli {
    #[arg(long, global = true, help = "Emit JSON structured output")]
    pub json: bool,

    #[arg(short, long, global = true, action = clap::ArgAction::Count, help = "Increase verbosity (-v, -vv, -vvv)")]
    pub verbose: u8,

    #[arg(long, global = true, help = "Override VM name")]
    pub vm_name: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Detect, build, deploy, and verify a driver
    Test(TestCommand),
    /// Create and configure a test VM
    Setup(SetupCommand),
    /// Create or revert baseline snapshot
    Snapshot(SnapshotCommand),
    /// Deploy a driver to a VM
    Deploy(DeployCommand),
    /// Remove test VM
    Clean(CleanCommand),
}

// ── test ──────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct TestCommand {
    #[arg(long, help = "Path to driver package (default: cwd)")]
    pub package_path: Option<String>,
    #[arg(long, help = "Revert to baseline snapshot first")]
    pub revert_snapshot: bool,
    #[arg(long, help = "Capture debug output")]
    pub capture_output: bool,
    #[arg(long, help = "Override detected driver type (KMDF|UMDF|WDM)")]
    pub driver_type: Option<String>,
}

impl TestCommand {
    pub fn run(&self, vm_name: Option<&str>, json: bool) -> Result<()> {
        let vm_name = vm_name.unwrap_or("driver-test-vm");
        let provider = HypervProvider::default();
        let deployer = PnpDeployer;

        let pkg_root = PathBuf::from(self.package_path.as_deref().unwrap_or("."));
        let progress = Progress::new("Detecting driver package", !json);

        let package = detect::detect_driver(&pkg_root, self.driver_type.as_deref())?;
        progress.done(&format!("{} driver detected", package.driver_type));

        // Get/ensure VM
        let vm = provider
            .get_vm(vm_name)?
            .ok_or_else(|| AppError::Vm(format!("VM '{vm_name}' not found. Run 'driver-test-cli setup' first.")))?;

        if self.revert_snapshot {
            let snap = crate::vm::SnapshotId("baseline".into());
            provider.revert_snapshot(&vm, &snap)?;
        }

        let vm = provider.ensure_running(&vm)?;

        // Start debug capture before deployment
        let mut capture_session = if self.capture_output {
            let cfg = CaptureConfig::default();
            match debug::start_capture(&provider, &vm, cfg) {
                Ok(session) => Some(session),
                Err(e) => {
                    tracing::warn!("debug capture failed to start: {e}");
                    None
                }
            }
        } else {
            None
        };

        // Deploy
        let deploy_progress = Progress::new("Deploying driver", !json);
        let inf = package.inf_path.as_deref().ok_or_else(|| {
            AppError::Detection("no INF file found for package".into())
        })?;

        let install = deploy::deploy_driver(
            &deployer,
            &provider,
            &vm,
            None, // cert — TODO: detect from build output
            inf,
            package.version.as_deref(),
        )?;
        deploy_progress.done("Driver installed");

        // Collect debug messages
        let debug_messages = if let Some(session) = capture_session.take() {
            Some(debug::stop_capture(&provider, &vm, session)?)
        } else {
            None
        };

        let result = TestResult {
            success: true,
            driver_type: Some(package.driver_type.to_string()),
            published_name: install.published_name,
            version: install.version,
            wmi: None,
            debug_messages,
            companion_output: None,
            error: None,
        };

        output::emit_result(&result, json);
        Ok(())
    }
}

// ── setup ─────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SetupCommand {
    #[arg(long, help = "VM name", default_value = "driver-test-vm")]
    pub vm_name: String,
    #[arg(long, help = "Memory (MB)", default_value = "4096")]
    pub memory_mb: u32,
    #[arg(long, help = "CPU count", default_value = "4")]
    pub cpu_count: u8,
}

impl SetupCommand {
    pub fn run(&self) -> Result<()> {
        let provider = HypervProvider::default();

        if let Some(existing) = provider.get_vm(&self.vm_name)? {
            println!("VM '{}' already exists (state: {})", existing.name, existing.state);
            return Ok(());
        }

        let progress = Progress::new(format!("Creating VM '{}'", self.vm_name), true);
        let vm = provider.create_vm(&self.vm_name, self.memory_mb, self.cpu_count)?;
        progress.done("VM created");

        println!("VM '{}' created:", vm.name);
        println!("  Memory: {} MB", vm.memory_mb);
        println!("  CPUs: {}", vm.cpus);
        println!("\nNext steps:");
        println!("  1. Install Windows on the VM");
        println!("  2. In the guest, run: bcdedit /set testsigning on");
        println!("  3. Disable Secure Boot in VM settings (for Gen 2 VMs)");
        println!("  4. Enable Guest Service Interface");
        println!("  5. Start VM and create baseline: driver-test-cli snapshot --create");
        Ok(())
    }
}

// ── snapshot ──────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct SnapshotCommand {
    #[arg(long, help = "Create baseline snapshot")]
    pub create: bool,
    #[arg(long, help = "Revert to baseline snapshot")]
    pub revert: bool,
}

impl SnapshotCommand {
    pub fn run(&self, vm_name: Option<&str>) -> Result<()> {
        if !self.create && !self.revert {
            anyhow::bail!("specify --create or --revert");
        }
        let vm_name = vm_name.unwrap_or("driver-test-vm");
        let provider = HypervProvider::default();
        let vm = provider
            .get_vm(vm_name)?
            .ok_or_else(|| anyhow::anyhow!("VM '{vm_name}' not found"))?;

        if self.create {
            let snap = provider.snapshot(&vm, "baseline")?;
            println!("Baseline snapshot '{}' created for '{}'", snap.0, vm.name);
        }
        if self.revert {
            let snap = crate::vm::SnapshotId("baseline".into());
            let vm = provider.revert_snapshot(&vm, &snap)?;
            println!("VM '{}' reverted to baseline (state: {})", vm.name, vm.state);
        }
        Ok(())
    }
}

// ── deploy ────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct DeployCommand {
    #[arg(long, help = "Path to INF file")]
    pub inf: String,
    #[arg(long, help = "Certificate (.cer) path")]
    pub cert: Option<String>,
    #[arg(long, help = "Expected driver version")]
    pub expected_version: Option<String>,
    #[arg(long, help = "Query WMI metadata")]
    pub wmi: bool,
}

impl DeployCommand {
    pub fn run(&self, vm_name: Option<&str>, json: bool) -> Result<()> {
        let vm_name = vm_name.unwrap_or("driver-test-vm");
        let provider = HypervProvider::default();
        let deployer = PnpDeployer;

        let vm = provider
            .get_vm(vm_name)?
            .ok_or_else(|| AppError::Vm(format!("VM '{vm_name}' not found")))?;
        let vm = provider.ensure_running(&vm)?;

        let inf_path = Path::new(&self.inf);
        let cert_path = self.cert.as_ref().map(Path::new);

        let install = deploy::deploy_driver(
            &deployer,
            &provider,
            &vm,
            cert_path,
            inf_path,
            self.expected_version.as_deref(),
        )?;

        let wmi = if self.wmi {
            if let Some(ref name) = install.published_name {
                deploy::query_wmi(&vm, name).ok().flatten()
            } else {
                None
            }
        } else {
            None
        };

        let result = TestResult {
            success: true,
            driver_type: None,
            published_name: install.published_name,
            version: install.version,
            wmi,
            debug_messages: None,
            companion_output: None,
            error: None,
        };

        output::emit_result(&result, json);
        Ok(())
    }
}

// ── clean ─────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct CleanCommand {
    #[arg(long, help = "Skip confirmation")]
    pub yes: bool,
}

impl CleanCommand {
    pub fn run(&self, vm_name: Option<&str>) -> Result<()> {
        let vm_name = vm_name.unwrap_or("driver-test-vm");
        let provider = HypervProvider::default();

        let vm = match provider.get_vm(vm_name)? {
            Some(v) => v,
            None => {
                println!("VM '{vm_name}' not found");
                return Ok(());
            }
        };

        if !self.yes {
            eprint!("Remove VM '{}' and all data? [y/N]: ", vm.name);
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Cancelled");
                return Ok(());
            }
        }

        provider.remove_vm(&vm)?;
        println!("VM '{}' removed", vm.name);
        Ok(())
    }
}
