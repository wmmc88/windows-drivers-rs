// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    env,
    path::{Path, PathBuf},
};

use bindgen::CodegenConfig;
use wdk_build::{BuilderExt, Config, ConfigError, DriverConfig, KMDFConfig};

// FIXME: feature gate the WDF version
// FIXME: check that the features are exclusive
// const KMDF_VERSIONS: &'static [&'static str] = &[
//     "1.9", "1.11", "1.13", "1.15", "1.17", "1.19", "1.21", "1.23", "1.25",
// "1.27", "1.31", "1.33", ];
// const UMDF_VERSIONS: &'static [&'static str] = &[
//     "2.0", "2.15", "2.17", "2.19", "2.21", "2.23", "2.25", "2.27", "2.31",
// "2.33", ];

fn generate_types(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(
        vec![
            "src/ntddk-input.h",
            "src/hid-input.h",
            "src/wdf-input.h",
            "src/usb-input.h",
            "src/parallel-ports-input.h",
            "src/spb-input.h"
        ],
        config,
    )?
    .with_codegen_config(CodegenConfig::TYPES)
    .generate()
    .expect("Bindings should succeed to generate")
    .write_to_file(out_path.join("types.rs"))?)
}
fn generate_constants(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(bindgen::Builder::wdk_default(
        vec![
            "src/ntddk-input.h",
            "src/hid-input.h",
            "src/wdf-input.h",
            "src/usb-input.h",
            "src/parallel-ports-input.h",
            "src/spb-input.h"
        ],
        config,
    )?
    .with_codegen_config(CodegenConfig::VARS)
    .generate()
    .expect("Bindings should succeed to generate")
    .write_to_file(out_path.join("constants.rs"))?)
}

fn generate_ntddk(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    Ok(
        bindgen::Builder::wdk_default(vec!["src/ntddk-input.h"], config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("ntddk.rs"))?,
    )
}

fn generate_hid(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    let mut builder = bindgen::Builder::wdk_default(vec!["src/hid-input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement());

    // Only allowlist files in the hid-specific files declared in hid-input.h to
    // avoid duplicate definitions
    for header_file in [
        "hidclass.h",
        "hidpddi.h",
        "hidpi.h",
        "hidport.h",
        "hidsdi.h",
        "hidspicx.h",
        "kbdmou.h",
        "ntdd8042.h",
        "vhf.h",
    ] {
        builder = builder.allowlist_file(format!(".*{header_file}.*"));
    }

    Ok(builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("hid.rs"))?)
}

fn generate_parallel_ports(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    let mut builder = bindgen::Builder::wdk_default(vec!["src/parallel-ports-input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement());

    // Only allowlist files in the parallel ports-specific files declared in
    // parallel-ports-input.h to avoid duplicate definitions
    for header_file in [
        "gpio.h",
        "gpioclx.h",
        "ntddpar.h",
        "ntddser.h",
        "parallel.h",
    ] {
        builder = builder.allowlist_file(format!(".*{header_file}.*"));
    }

    Ok(builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("parallel_ports.rs"))?)
}

fn generate_wdf(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    // As of NI WDK, this may generate an empty file due to no non-type and non-var
    // items in the wdf headers(i.e. functions are all inlined). This step is
    // intentionally left here in case older WDKs have non-inlined functions or new
    // WDKs may introduce non-inlined functions.
    Ok(
        bindgen::Builder::wdk_default(vec!["src/wdf-input.h"], config)?
            .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement())
            .allowlist_file("(?i).*wdf.*") // Only generate for files that are prefixed with (case-insensitive) wdf (ie.
            // /some/path/WdfSomeHeader.h), to prevent duplication of code in ntddk.rs
            .generate()
            .expect("Bindings should succeed to generate")
            .write_to_file(out_path.join("wdf.rs"))?,
    )
}

fn generate_usb(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    let mut builder = bindgen::Builder::wdk_default(vec!["src/usb-input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement());

    // Only allowlist files in the usb-specific files declared in usb-input.h to
    // avoid duplicate definitions
    for header_file in [
        "usb.h",
        "usbbusif.h",
        "usbdlib.h",
        "usbfnattach.h",
        "usbfnbase.h",
        "usbfnioctl.h",
        "usbioctl.h",
        "usbspec.h",
    ] {
        builder = builder.allowlist_file(format!(".*{header_file}.*"));
    }

    Ok(builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("usb.rs"))?)
}

fn generate_spb(out_path: &Path, config: Config) -> Result<(), ConfigError> {
    let mut builder = bindgen::Builder::wdk_default(vec!["src/spb-input.h"], config)?
        .with_codegen_config((CodegenConfig::TYPES | CodegenConfig::VARS).complement());

    // Only allowlist files in the usb-specific files declared in spb-input.h to
    // avoid duplicate definitions
    for header_file in [
        "spb.h",
        "spbcx.h",
        "reshub.h",
        "pwmutil.h",
    ] {
        builder = builder.allowlist_file(format!(".*{header_file}.*"));
    }

    Ok(builder
        .generate()
        .expect("Bindings should succeed to generate")
        .write_to_file(out_path.join("spb.rs"))?)
}

fn main() -> Result<(), ConfigError> {
    tracing_subscriber::fmt::init();

    let config = Config {
        // FIXME: this should be based off of Cargo feature version
        driver_config: DriverConfig::KMDFConfig(KMDFConfig::new()),
        ..Config::default()
    };

    let out_paths = vec![
        // FIXME: gate the generations of the generated_bindings folder behind a feature flag that
        // is disabled in crates.io builds (modifying source is illegal when distributing
        // crates)

        // Generate a copy of the bindings to the generated_bindings so that its easier to see
        // diffs in the output due to bindgen settings changes
        PathBuf::from("./generated_bindings/"),
        // This is the actual bindings that get consumed via !include in this library's modules
        PathBuf::from(
            env::var("OUT_DIR").expect("OUT_DIR should be exist in Cargo build environment"),
        ),
    ];

    for out_path in out_paths {
        generate_types(&out_path, config.clone())?;
        generate_constants(&out_path, config.clone())?;
        generate_ntddk(&out_path, config.clone())?;
        generate_wdf(&out_path, config.clone())?;
        generate_hid(&out_path, config.clone())?;
        generate_usb(&out_path, config.clone())?;
        generate_parallel_ports(&out_path, config.clone())?;
        generate_spb(&out_path, config.clone())?;
    }

    config.configure_library_build()?;
    Ok(config.export_config()?)
}
