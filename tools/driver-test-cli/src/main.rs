mod cli;
mod config;
mod debug;
mod deploy;
mod detect;
mod echo;
mod errors;
mod output;
mod ps;
mod vm;

use clap::Parser;
use cli::Cli;
use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_tracing(verbosity: u8, json: bool) {
    let level = match verbosity {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let filter = EnvFilter::new(level.to_string());
    if json {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json().flatten_event(true).with_target(false))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(false))
            .init();
    }
}

fn main() {
    let cli = Cli::parse();

    // Load optional config
    let config_path = std::env::var("DRIVER_TEST_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(config::DEFAULT_CONFIG_FILE));
    let config = config::Config::maybe_load(&config_path);

    // Resolve VM name: CLI flag > config file > default
    let vm_name = cli
        .vm_name
        .clone()
        .or_else(|| config.as_ref().and_then(|c| c.vm_name().map(String::from)));

    init_tracing(cli.verbose, cli.json);

    let vm_ref = vm_name.as_deref();
    let result = match cli.command {
        cli::Commands::Test(cmd) => cmd.run(vm_ref, cli.json),
        cli::Commands::Setup(cmd) => cmd.run(),
        cli::Commands::Snapshot(cmd) => cmd.run(vm_ref),
        cli::Commands::Deploy(cmd) => cmd.run(vm_ref, cli.json),
        cli::Commands::Clean(cmd) => cmd.run(vm_ref),
    };

    if let Err(e) = result {
        if cli.json {
            let err_result = output::TestResult {
                success: false,
                driver_type: None,
                published_name: None,
                version: None,
                wmi: None,
                debug_messages: None,
                companion_output: None,
                error: Some(e.to_string()),
            };
            output::emit_result(&err_result, true);
        } else {
            eprintln!("ERROR: {e}");
            if let Some(app_err) = e.downcast_ref::<errors::AppError>() {
                match app_err {
                    errors::AppError::Vm(msg) if msg.contains("not found") => {
                        eprintln!("  ACTION: Run 'driver-test-cli setup' to create a test VM");
                    }
                    errors::AppError::Prerequisite(msg) => {
                        eprintln!("  ACTION: {msg}");
                    }
                    _ => {}
                }
            }
        }
        let code = e
            .downcast_ref::<errors::AppError>()
            .map(|a| a.exit_code())
            .unwrap_or(1);
        std::process::exit(code);
    }
}
