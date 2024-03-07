// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! A collection of macros that help make it easier to interact with
//! [`wdk-sys`]'s direct bindings to the Windows Driver Kit (WDK).
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::multiple_unsafe_ops_per_block)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::unnecessary_safety_doc)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::invalid_rust_codeblocks)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::unescaped_backticks)]
#![deny(rustdoc::redundant_explicit_links)]

use std::{
    io::{BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
};

use cargo_metadata::{Message, MetadataCommand, PackageId};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2,
    punctuated::Punctuated,
    spanned::Spanned,
    BareFnArg,
    Error,
    Expr,
    Ident,
    Path,
    PathSegment,
    ReturnType,
    Token,
    Type,
    TypePath,
};

/// A procedural macro that allows WDF functions to be called by name.
///
/// This function parses the name of the WDF function, finds it function pointer
/// from the WDF function table, and then calls it with the arguments passed to
/// it
///
/// # Safety
/// Function arguments must abide by any rules outlined in the WDF
/// documentation. This macro does not perform any validation of the arguments
/// passed to it., beyond type validation.
///
/// # Examples
///
/// ```rust, no_run
/// use wdk_sys::*;
///
/// #[export_name = "DriverEntry"]
/// pub extern "system" fn driver_entry(
///     driver: &mut DRIVER_OBJECT,
///     registry_path: PCUNICODE_STRING,
/// ) -> NTSTATUS {
///     let mut driver_config = WDF_DRIVER_CONFIG {
///         Size: core::mem::size_of::<WDF_DRIVER_CONFIG>() as ULONG,
///         ..WDF_DRIVER_CONFIG::default()
///     };
///     let driver_handle_output = WDF_NO_HANDLE as *mut WDFDRIVER;
///
///     unsafe {
///         wdk_macros::call_unsafe_wdf_function_binding!(
///             WdfDriverCreate,
///             driver as PDRIVER_OBJECT,
///             registry_path,
///             WDF_NO_OBJECT_ATTRIBUTES,
///             &mut driver_config,
///             driver_handle_output,
///         )
///     }
/// }
/// ```
#[allow(clippy::unnecessary_safety_doc)]
#[proc_macro]
pub fn call_unsafe_wdf_function_binding(input_tokens: TokenStream) -> TokenStream {
    call_unsafe_wdf_function_binding_impl(TokenStream2::from(input_tokens)).into()
}

struct CallUnsafeWDFFunctionParseOutputs {
    function_pointer_type: Ident,
    function_table_index: Ident,
    parameters: Punctuated<BareFnArg, Token![,]>,
    return_type: ReturnType,
    arguments: Punctuated<Expr, Token![,]>,
}

impl Parse for CallUnsafeWDFFunctionParseOutputs {
    fn parse(input: ParseStream) -> Result<Self, Error> {
        // parse inputs
        let c_function_name: String = input.parse::<Ident>()?.to_string();
        input.parse::<Token![,]>()?;
        let arguments = input.parse_terminated(Expr::parse, Token![,])?;

        // compute parse outputs
        let function_pointer_type = format_ident!(
            "PFN_{uppercase_c_function_name}",
            uppercase_c_function_name = c_function_name.to_uppercase()
        );
        let function_table_index = format_ident!("{c_function_name}TableIndex");
        let (parameters, return_type) =
            compute_parse_outputs_from_generated_code(&function_pointer_type)?;

        Ok(Self {
            function_pointer_type,
            function_table_index,
            parameters,
            return_type,
            arguments,
        })
    }
}

fn call_unsafe_wdf_function_binding_impl(input_tokens: TokenStream2) -> TokenStream2 {
    let CallUnsafeWDFFunctionParseOutputs {
        function_pointer_type,
        function_table_index,
        parameters,
        return_type,
        arguments,
    } = match parse2::<CallUnsafeWDFFunctionParseOutputs>(input_tokens) {
        Ok(syntax_tree) => syntax_tree,
        Err(err) => return err.to_compile_error(),
    };

    let wdf_function_call_tokens = generate_wdf_function_call_tokens(
        &function_pointer_type,
        &function_table_index,
        &parameters,
        &return_type,
        &arguments,
    );

    let must_use_attribute = if matches!(return_type, ReturnType::Type(..)) {
        quote! { #[must_use] }
    } else {
        TokenStream2::new()
    };

    quote! {
        {
            // TODO: remove this
            use wdk_sys::*;

            // Force the macro to require an unsafe block
            unsafe fn force_unsafe(){}
            force_unsafe();

            #must_use_attribute
            #wdf_function_call_tokens
        }
    }
}

/// Compute the function parameters and return type corresponding to the
/// function signature of the function_pointer_type type alias in the AST for
/// types.rs
fn compute_parse_outputs_from_generated_code(
    function_pointer_type: &Ident,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType), Error> {
    let types_rs_ast = get_abstract_syntax_tree_from_types_rs()?;
    let type_alias_definition = find_type_alias_definition(&types_rs_ast, function_pointer_type)?;
    let fn_pointer_definition = extract_fn_pointer_definition(type_alias_definition)?;
    Ok(compute_parse_outputs_from_fn_pointer_definition(
        fn_pointer_definition,
    )?)
}

fn get_abstract_syntax_tree_from_types_rs() -> Result<syn::File, Error> {
    let types_rs_path = find_wdk_sys_out_dir()?.join("types.rs");
    let types_rs_contents = match std::fs::read_to_string(&types_rs_path) {
        Ok(contents) => contents,
        Err(err) => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "Failed to read wdk-sys types.rs at {}: {}",
                    types_rs_path.display(),
                    err
                ),
            ));
        }
    };

    match syn::parse_file(&types_rs_contents) {
        Ok(wdk_sys_types_rs_abstract_syntax_tree) => Ok(wdk_sys_types_rs_abstract_syntax_tree),
        Err(err) => Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "Failed to parse wdk-sys types.rs into AST at {}: {}",
                types_rs_path.display(),
                err
            ),
        )),
    }
}

fn find_wdk_sys_out_dir() -> Result<PathBuf, Error> {
    let mut cargo_check_process_handle = match Command::new("cargo")
        .args([
            "check",
            "--message-format=json",
            "--package",
            "wdk-sys",
            // must have a seperate target directory to prevent deadlock from cargo holding a
            // file lock on build output directory since this proc_macro causes
            // cargo build to invoke cargo check
            "--target-dir",
            "target/wdk-macros-target",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(err) => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("Failed to start cargo check process successfully: {err}"),
            ));
        }
    };

    let wdk_sys_pkg_id = find_wdk_sys_pkg_id()?;
    let wdk_sys_out_dir = cargo_metadata::Message::parse_stream(BufReader::new(
        cargo_check_process_handle
            .stdout
            .take()
            .expect("cargo check process should have valid stdout handle"),
    ))
    .filter_map(|message| {
        if let Ok(Message::BuildScriptExecuted(build_script_message)) = message {
            if build_script_message.package_id == wdk_sys_pkg_id {
                return Some(build_script_message.out_dir);
            }
        }
        None
    })
    .collect::<Vec<_>>();
    let wdk_sys_out_dir = match wdk_sys_out_dir.len() {
        1 => &wdk_sys_out_dir[0],
        _ => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "Expected exactly one instance of wdk-sys in dependency graph, found {}",
                    wdk_sys_out_dir.len()
                ),
            ));
        }
    };
    match cargo_check_process_handle.wait() {
        Ok(exit_status) => {
            if !exit_status.success() {
                let mut stderr_output = String::new();
                BufReader::new(
                    cargo_check_process_handle
                        .stderr
                        .take()
                        .expect("cargo check process should have valid stderr handle"),
                )
                .read_to_string(&mut stderr_output)
                .expect("cargo check process' stderr should be valid UTF-8");
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "cargo check failed to execute to get OUT_DIR for wdk-sys: \
                         \n{stderr_output}"
                    ),
                ));
            }
        }
        Err(err) => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("cargo check process handle should sucessfully be waited on: {err}"),
            ));
        }
    }

    Ok(wdk_sys_out_dir.to_owned().into())
}

/// find wdk-sys package_id. WDR places a limitation that only one instance of
/// wdk-sys is allowed in the dependency graph
fn find_wdk_sys_pkg_id() -> Result<PackageId, Error> {
    let cargo_metadata_packages_list = match MetadataCommand::new().exec() {
        Ok(metadata) => metadata.packages,
        Err(err) => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("cargo metadata failed to run successfully: {err}"),
            ));
        }
    };
    let wdk_sys_package_matches = cargo_metadata_packages_list
        .iter()
        .filter(|package| package.name == "wdk-sys")
        .collect::<Vec<_>>();

    if wdk_sys_package_matches.len() != 1 {
        return Err(Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "Expected exactly one instance of wdk-sys in dependency graph, found {}",
                wdk_sys_package_matches.len()
            ),
        ));
    }
    Ok(wdk_sys_package_matches[0].id.clone())
}

/// Find type alias definition that matches the Ident of `function_pointer_type`
/// in syn::File AST
///
/// For example, passing the `PFN_WDFDRIVERCREATE` [`Ident`] as
/// `function_pointer_type` would return a [`ItemType`] representation of: ```
/// pub type PFN_WDFDRIVERCREATE = ::core::option::Option<
///     unsafe extern "C" fn(
///         DriverGlobals: PWDF_DRIVER_GLOBALS,
///         DriverObject: PDRIVER_OBJECT,
///         RegistryPath: PCUNICODE_STRING,
///         DriverAttributes: PWDF_OBJECT_ATTRIBUTES,
///         DriverConfig: PWDF_DRIVER_CONFIG,
///         Driver: *mut WDFDRIVER,
///     ) -> NTSTATUS,
/// >;
/// ```
fn find_type_alias_definition<'a>(
    ast: &'a syn::File,
    function_pointer_type: &Ident,
) -> Result<&'a syn::ItemType, Error> {
    ast.items
        .iter()
        .find_map(|item| {
            if let syn::Item::Type(type_alias) = item {
                if type_alias.ident == *function_pointer_type {
                    return Some(type_alias);
                }
            }
            None
        })
        .ok_or_else(|| {
            Error::new(
                function_pointer_type.span(),
                format!("Failed to find type alias definition for {function_pointer_type}"),
            )
        })
}

fn extract_fn_pointer_definition(type_alias: &syn::ItemType) -> Result<&syn::TypePath, Error> {
    if let syn::Type::Path(fn_pointer) = type_alias.ty.as_ref() {
        Ok(fn_pointer)
    } else {
        Err(syn::Error::new(type_alias.ty.span(), "Expected Type::Path"))
    }
}

fn compute_parse_outputs_from_fn_pointer_definition(
    fn_pointer_definition: &syn::TypePath,
) -> Result<(Punctuated<BareFnArg, Token![,]>, ReturnType), Error> {
    let Some(syn::PathSegment {
        ident: option_ident,
        arguments: option_arguments,
        ..
    }) = fn_pointer_definition.path.segments.last()
    else {
        return Err(Error::new(
            fn_pointer_definition.path.segments.span(),
            "Expected PathSegments",
        ));
    };

    if option_ident != "Option" {
        return Err(Error::new(option_ident.span(), "Expected Option"));
    }

    let syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
        args: option_angle_bracketed_args,
        ..
    }) = option_arguments
    else {
        return Err(Error::new(
            option_arguments.span(),
            "Expected AngleBracketed PathArguments",
        ));
    };
    if option_angle_bracketed_args.len() != 1 {
        return Err(Error::new(
            option_angle_bracketed_args.span(),
            "Expected exactly one generic argument",
        ));
    }

    let Some(syn::GenericArgument::Type(syn::Type::BareFn(syn::TypeBareFn {
        inputs: fn_parameters,
        output: fn_return_type,
        ..
    }))) = option_angle_bracketed_args.first()
    else {
        return Err(Error::new(
            option_angle_bracketed_args.span(),
            "Expected BareFn",
        ));
    };
    if fn_parameters.is_empty() {
        return Err(Error::new(
            fn_parameters.span(),
            "Expected at least one function parameter",
        ));
    }

    let Some(BareFnArg {
        ty:
            Type::Path(TypePath {
                path:
                    Path {
                        segments: first_parameter_type_path,
                        ..
                    },
                ..
            }),
        ..
    }) = fn_parameters.first()
    else {
        return Err(Error::new(
            fn_parameters.span(),
            // todo
            "Expected BareFnArg",
        ));
    };

    let Some(PathSegment {
        ident: first_parameter_type_identifier,
        ..
    }) = first_parameter_type_path.last()
    else {
        return Err(Error::new(
            first_parameter_type_path.span(),
            "Expected PathSegment",
        ));
    };
    if first_parameter_type_identifier != "PWDF_DRIVER_GLOBALS" {
        return Err(Error::new(
            first_parameter_type_identifier.span(),
            "Expected PWDF_DRIVER_GLOBALS",
        ));
    }

    // todo: add wdk_sys::
    let parameters = fn_parameters
        .iter()
        .skip(1) // skip PWDF_DRIVER_GLOBALS
        .cloned()
        .collect();

    let return_type = match fn_return_type {
        ReturnType::Default => ReturnType::Default,
        ReturnType::Type(right_arrow_token, ty) => {
            let Type::Path(ref type_path) = **ty else {
                return Err(Error::new(ty.span(), "Expected Type::Path"));
            };

            let mut modified_type_path = type_path.clone();
            modified_type_path.path.segments.insert(
                0,
                PathSegment {
                    ident: format_ident!("wdk_sys"),
                    arguments: syn::PathArguments::None,
                },
            );

            ReturnType::Type(*right_arrow_token, Box::new(Type::Path(modified_type_path)))
        }
    };

    return Ok((parameters, return_type));
}

fn generate_wdf_function_call_tokens(
    function_pointer_type: &Ident,
    function_table_index: &Ident,
    parameters: &Punctuated<BareFnArg, Token![,]>,
    return_type: &ReturnType,
    arguments: &Punctuated<Expr, Token![,]>,
) -> TokenStream2 {
    let parameter_identifiers: Punctuated<Ident, Token![,]> = parameters
        .iter()
        .cloned()
        .filter_map(|bare_fn_arg| {
            if let Some((identifier, _)) = bare_fn_arg.name {
                return Some(identifier);
            }
            None
            // TODO error
        })
        .collect();

    quote! {
        #[inline(always)]
        // TODO: fix fn name
        fn unsafe_imp(#parameters) #return_type {
            // Get handle to WDF function from the function table
            let wdf_function: wdk_sys::#function_pointer_type = Some(
                // SAFETY: This `transmute` from a no-argument function pointer to a function pointer with the correct
                //         arguments for the WDF function is safe befause WDF maintains the strict mapping between the
                //         function table index and the correct function pointer type.
                #[allow(unused_unsafe)]
                #[allow(clippy::multiple_unsafe_ops_per_block)]
                unsafe {
                    core::mem::transmute(
                        // FIXME: investigate why _WDFFUNCENUM does not have a generated type alias without the underscore prefix
                        wdk_sys::WDF_FUNCTION_TABLE[wdk_sys::_WDFFUNCENUM::#function_table_index as usize],
                    )
                }
            );

            // Call the WDF function with the supplied args. This mirrors what happens in the inlined WDF function in
            // the various wdf headers(ex. wdfdriver.h)
            if let Some(wdf_function) = wdf_function {
                // SAFETY: The WDF function pointer is always valid because its an entry in
                // `wdk_sys::WDF_FUNCTION_TABLE` indexed by `table_index` and guarded by the type-safety of
                // `pointer_type`. The passed arguments are also guaranteed to be of a compatible type due to
                // `pointer_type`.
                #[allow(unused_unsafe)]
                #[allow(clippy::multiple_unsafe_ops_per_block)]
                unsafe {
                    (wdf_function)(
                        wdk_sys::WdfDriverGlobals,
                        #parameter_identifiers
                    )
                }
            } else {
                unreachable!("Option should never be None");
            }
        }

        unsafe_imp(#arguments)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use lazy_static::lazy_static;

    lazy_static! {
        static ref TESTS_FOLDER_PATH: PathBuf =
            [env!("CARGO_MANIFEST_DIR"), "tests"].iter().collect();
        static ref MACROTEST_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("macrotest");
        static ref TRYBUILD_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("trybuild");
    }

    mod macro_expansion_and_compilation {
        use std::{io::Write, stringify};

        use owo_colors::OwoColorize;
        use paste::paste;

        use super::*;

        /// Given a filename `f` which contains code utilizing macros in
        /// `wdk-macros`, generates a pair of tests to verify that code in `f`
        /// expands as expected, and compiles successfully. The test output will
        /// show `<f>_expansion` as the names of the expansion tests and
        /// `<f>_compilation` as the name of the compilation test. `f` must
        /// reside in the `tests/macrotest` folder, and may be a path to
        /// a file relative to the `tests/macrotest` folder.
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
        macro_rules! generate_macro_expansion_and_compilation_tests {
            ($($filename:ident),+) => {
                paste! {

                    // This module's tests are deliberately not feature-gated by #[cfg(feature = "nightly")] since macrotest can control whether to expand with the nightly feature or not
                    mod expansion_tests {
                        use super::*;

                        $(
                            #[test]
                            fn [<$filename _expansion>]() -> std::io::Result<()> {
                                macrotest::expand(&MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?);
                                Ok(())
                            }
                        )?

                        mod nightly_feature {
                            use super::*;

                            $(
                                #[test]
                                fn [<$filename _expansion>]() -> std::io::Result<()> {
                                    macrotest::expand_args(
                                        &MACROTEST_FOLDER_PATH.join(format!("{}.rs", stringify!($filename))).canonicalize()?, &["--features", "nightly"]);
                                    Ok(())
                                }
                            )?
                        }
                    }

                    mod compilation_tests {
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
                                trybuild::TestCases::pass_cargo_check(
                                    &MACROTEST_FOLDER_PATH
                                        .join(format!("{}.rs", stringify!($filename)))
                                        .canonicalize()
                                        .expect(concat!(stringify!($filename), " should exist")),
                                );
                            }
                        )?

                        #[cfg(feature = "nightly")]
                        mod nightly_feature {
                            use super::*;

                            $(
                                #[test]
                                fn [<$filename _compilation>]() {
                                    trybuild::TestCases::pass_cargo_check(
                                        &MACROTEST_FOLDER_PATH
                                            .join(format!("{}.rs", stringify!($filename)))
                                            .canonicalize()
                                            .expect(concat!(stringify!($filename), " should exist")),
                                    );
                                }
                            )?
                        }
                    }
                }
            };
        }

        generate_macro_expansion_and_compilation_tests!(
            wdf_driver_create,
            wdf_device_create,
            wdf_device_create_device_interface,
            wdf_spin_lock_acquire
        );
    }

    mod macro_usage_errors {
        use super::*;

        /// This test leverages `trybuild` to ensure that developer misuse of
        /// the macro cause compilation failures, with an appropriate message
        #[test]
        fn trybuild() {
            trybuild::TestCases::new().compile_fail(
                // canonicalization of this path causes a bug in `glob`: https://github.com/rust-lang/glob/issues/132
                TRYBUILD_FOLDER_PATH // .canonicalize()?
                    .join("*.rs"),
            );
        }
    }
}
