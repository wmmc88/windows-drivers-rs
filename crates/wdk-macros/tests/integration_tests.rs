// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use fs4::FileExt;
use lazy_static::lazy_static;

#[rustversion::stable]
const TOOLCHAIN_CHANNEL_NAME: &str = "stable";

#[rustversion::beta]
const TOOLCHAIN_CHANNEL_NAME: &str = "beta";

#[rustversion::nightly]
const TOOLCHAIN_CHANNEL_NAME: &str = "nightly";

lazy_static! {
    static ref TESTS_FOLDER_PATH: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests"].iter().collect();
    static ref INPUTS_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("inputs");
    static ref MACROTEST_INPUT_FOLDER_PATH: PathBuf = INPUTS_FOLDER_PATH.join("macrotest");
    static ref TRYBUILD_INPUT_FOLDER_PATH: PathBuf = INPUTS_FOLDER_PATH.join("trybuild");
    static ref OUTPUTS_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("outputs");
    static ref TOOLCHAIN_SPECIFIC_OUTPUTS_FOLDER_PATH: PathBuf =
        OUTPUTS_FOLDER_PATH.join(TOOLCHAIN_CHANNEL_NAME);
    static ref MACROTEST_OUTPUT_FOLDER_PATH: PathBuf =
        TOOLCHAIN_SPECIFIC_OUTPUTS_FOLDER_PATH.join("macrotest");
    static ref TRYBUILD_OUTPUT_FOLDER_PATH: PathBuf =
        TOOLCHAIN_SPECIFIC_OUTPUTS_FOLDER_PATH.join("trybuild");
}

use std::{io::Write, stringify};

use owo_colors::OwoColorize;
use paste::paste;

/// Given a filename `f` which contains code utilizing macros in
/// `wdk-macros`, generates a pair of tests to verify that code in `f`
/// expands as expected, and compiles successfully. The test output will
/// show `<f>_expansion` as the names of the expansion tests and
/// `<f>_compilation` as the name of the compilation test. `f` must
/// reside in the `tests/inputs/macrotest` folder, and may be a path to
/// a file relative to the `tests/inputs/macrotest` folder. This macro is
/// designed to use one test file per generated test to fully take advantage of
/// parallization of tests in cargo.
///
/// Note: Due to limitations in `trybuild`, a successful compilation
/// test will include output that looks similar to the following:
/// ```
/// test \\?\D:\git-repos\windows-drivers-rs\crates\wdk-macros\tests\macrotest\wdf_driver_create.rs ... error
/// Expected test case to fail to compile, but it succeeded.
/// ```
/// This is because `trybuild` will run `cargo check` when calling
/// `TestCases::compile_fail`, but will run `cargo build` if calling
/// `TestCases::pass`. `cargo build` will fail at link stage due to
/// `trybuild` not allowing configuration to compile as a`cdylib`. To
/// work around this, `compile_fail` is used, and we mark the test as
/// expecting to panic with a specific message using the `should_panic`
/// attribute macro.
macro_rules! generate_macrotest_tests {
    ($($filename:ident),+) => {
        paste! {

            // This module's tests are deliberately not feature-gated by #[cfg(feature = "nightly")] and #[cfg(not(feature = "nightly"))] since macrotest can control whether to expand with the nightly feature or not
            mod macro_expansion {
                use super::*;

                $(
                    #[test]
                    fn [<$filename _expansion>]() {
                        let symlink_target = &MACROTEST_INPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize().expect("canonicalize of symlink target should succeed");
                        let symlink_path = &MACROTEST_OUTPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename)));
                        create_symlink_if_nonexistent(symlink_path, symlink_target);
                        macrotest::expand(symlink_path);
                    }
                )?

                mod nightly_feature {
                    use super::*;

                    $(
                        #[test]
                        fn [<$filename _expansion>]() {
                            let symlink_target = &MACROTEST_INPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize().expect("canonicalize of symlink target should succeed");
                            let symlink_path = &MACROTEST_OUTPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename)));
                            create_symlink_if_nonexistent(symlink_path, symlink_target);
                            macrotest::expand_args(
                                symlink_path, &["--features", "nightly"]);
                        }
                    )?
                }
            }

            mod macro_compilation {
                use super::*;

                pub trait TestCasesExt {
                    fn pass_cargo_check<P: AsRef<Path> + std::panic::UnwindSafe>(path: P);
                }

                impl TestCasesExt for trybuild::TestCases {
                    fn pass_cargo_check<P: AsRef<Path> + std::panic::UnwindSafe>(path: P) {
                        // "compile_fail" tests that pass cargo check result in this panic message
                        const SUCCESSFUL_CARGO_CHECK_STRING: &str = "1 of 1 tests failed";

                        let path = path.as_ref();

                        let failed_cargo_check = !std::panic::catch_unwind(|| {
                            // A new TestCases is required because it relies on running the tests upon drop
                            trybuild::TestCases::new().compile_fail(path);
                        })
                        .is_err_and(|cause| {
                            if let Some(str) = cause.downcast_ref::<&str>() {
                                *str == SUCCESSFUL_CARGO_CHECK_STRING
                            } else if let Some(string) = cause.downcast_ref::<String>() {
                                string == SUCCESSFUL_CARGO_CHECK_STRING
                            } else {
                                // Unexpected panic trait object type
                                false
                            }
                        });

                        if failed_cargo_check {
                            let failed_cargo_check_msg = format!(
                                "{}{}",
                                path.to_string_lossy().bold().red(),
                                " failed Cargo Check!".bold().red()
                            );

                            // Use writeln! to print even without passing --nocapture to the test harness
                            writeln!(&mut std::io::stderr(), "{failed_cargo_check_msg}").unwrap();

                            panic!("{failed_cargo_check_msg}");
                        } else {
                            // Use writeln! to print even without passing --nocapture to the test harness
                            writeln!(
                                &mut std::io::stderr(),
                                "{}{}{}{}{}",
                                "Please ignore the above \"Expected test case to fail to compile, but it \
                                succeeded.\" message (and its accompanying \"1 of 1 tests failed\" panic \
                                message when run with --nocapture).\n"
                                    .italic()
                                    .yellow(),
                                "test ".bold(),
                                path.to_string_lossy().bold(),
                                " ... ".bold(),
                                "PASSED".bold().green()
                            ).unwrap();
                        }
                    }
                }

                $(
                    #[cfg(not(feature = "nightly"))]
                    #[test]
                    fn [<$filename _compilation>]() {
                        let symlink_target = &MACROTEST_INPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize().expect("canonicalize of symlink target should succeed");
                        let symlink_path = &MACROTEST_OUTPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename)));
                        create_symlink_if_nonexistent(symlink_path, symlink_target);
                        trybuild::TestCases::pass_cargo_check(symlink_path);
                    }
                )?

                #[cfg(feature = "nightly")]
                mod nightly_feature {
                    use super::*;

                    $(
                        #[test]
                        fn [<$filename _compilation>]() {
                            let symlink_target = &MACROTEST_INPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize().expect("canonicalize of symlink target should succeed");
                            let symlink_path = &MACROTEST_OUTPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename)));
                            create_symlink_if_nonexistent(symlink_path, symlink_target);
                            trybuild::TestCases::pass_cargo_check(symlink_path);
                        }
                    )?
                }
            }
        }
    };
}

macro_rules! generate_trybuild_tests {
    ($($filename:ident),+) => {
        mod macro_usage_errors {
            use super::*;

            /// This test leverages `trybuild` to ensure that developer misuse of
            /// the macro cause compilation failures, with an appropriate message
            $(
                #[test]
                fn $filename() {
                    let symlink_target = &TRYBUILD_INPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize().expect("canonicalize of symlink target should succeed");
                    let symlink_path = &TRYBUILD_OUTPUT_FOLDER_PATH.join(format!("{}.rs", stringify!($filename)));
                    create_symlink_if_nonexistent(symlink_path, symlink_target);
                    trybuild::TestCases::new().compile_fail(symlink_path);
                }
            )?
        }
    };

}

fn create_symlink_if_nonexistent(link: &Path, target: &Path) {
    let target_file = File::open(target).expect("target file should be successfully opened");
    target_file
        .lock_exclusive()
        .expect("exclusive lock should be successfully acquired");
    // Only create a new symlink if there isn't an existing one, or if the existing
    // one points to the wrong place
    if !link.exists() {
        std::os::windows::fs::symlink_file(target, link).expect("symlink creation should succeed");
    } else if !link.is_symlink()
        || std::fs::read_link(link).expect("read_link of symlink should succeed") != target
    {
        std::fs::remove_file(link).expect("stale symlink removal should succeed");
        std::os::windows::fs::symlink_file(target, link).expect("symlink creation should succeed");
    } else {
        // symlink already exists and points to the correct place
    }
}

generate_macrotest_tests!(
    wdf_driver_create,
    wdf_device_create,
    wdf_device_create_device_interface,
    wdf_spin_lock_acquire,
    wdf_verifier_dbg_break_point
);

generate_trybuild_tests!(
    wdf_api_that_does_not_exist,
    wdf_device_create_unused_return_type,
    wdf_driver_create_missing_arg,
    wdf_driver_create_wrong_arg_order,
    wdf_timer_create_missing_unsafe
);
