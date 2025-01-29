// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::{
    collections::HashSet, env, fmt, io::{Read, Write}, path::{Path, PathBuf}, process::{Command, Stdio}, sync::LazyLock
};

use anyhow::{anyhow, bail, Result};
use cargo_metadata::{camino::Utf8PathBuf, Message, MetadataCommand};
use console::Style;
use ignore::WalkBuilder;
use similar::{Algorithm, ChangeTag};
use tempfile::TempDir;
use tracing_subscriber::fmt::format::FmtSpan;
use wdk_build::PathExt;

static REPO_ROOT: LazyLock<PathBuf> = LazyLock::new(|| {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Repo root folder should exist 2 folder levels up from wdk-build's folder")
        .canonicalize()
        .expect("Repo root folder should exist and be a valid path")
});

// TODO OPTIONS:
// base
// other
// features
// ouptut dir
// repo url?? default to system got
// WDK CONFIG? default to having latest KMDF, UMDF, WDM

// detect and print system deps differences:
// - Windows Drivers Kit
// - LLVM Version

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .pretty()
        .with_span_events(FmtSpan::FULL)
        .init();

    // set output directory in target if executed within repo, otherwise use current
    // working directory
    let diff_output_dir = {
        let cwd = std::env::current_dir()?.canonicalize()?;
        let temp_dir_base_path = if cwd.starts_with(REPO_ROOT.as_path()) {
            REPO_ROOT.join("target")
        } else {
            cwd
        };
        TempDir::with_prefix_in("wdk-sys-bindings-diff-", &temp_dir_base_path)?
    };

    // create comparison subdirectories
    let base_dir = diff_output_dir.path().join("base"); // TODO: name by commit hash?
    std::fs::create_dir(&base_dir)?;
    let other_dir = diff_output_dir.path().join("other");

    // TODO: only do this if `base` is main
    // clone latest main branch of windows-drivers-rs into latest-main
    let _ = git2::Repository::clone(
        "https://github.com/microsoft/windows-drivers-rs.git",
        &base_dir,
    )?;

    // TODO: only do this if `other` is local

    // copy all non-gitignored files to `other_dir`
    for dir_entry in WalkBuilder::new(REPO_ROOT.as_path())
        .hidden(true)
        .follow_links(true)
        .require_git(false) // Apply gitignore, regardless if in a git repo
        .git_global(false) // Ignore global git ignores to prevent it from modfiying behavior
        .git_exclude(false) // Prevent local-only ignores from modifying behavior
        .build()
    {
        let dir_entry = dir_entry?;
        let dir_entry_path: &Path = dir_entry.path();
        let repo_root_relative_path: &Path = dir_entry_path.strip_prefix(REPO_ROOT.as_path())?;

        if dir_entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            let target_path = other_dir.join(repo_root_relative_path);
            std::fs::create_dir_all(
                target_path
                    .parent()
                    .expect("parent of target path should exist"),
            )?;
            std::fs::copy(dir_entry_path, target_path)?;
        }
    }

    // temporarily do not delete output dir... maybe should
    // always keep output dir on error?
    diff_output_dir.into_path();

    // inject WDK config into workspace
    inject_wdk_configuration(&base_dir)?;
    inject_wdk_configuration(&other_dir)?;

    // extract OUT_DIR from both repo copies
    let base_wdk_sys_out_dir = extract_out_dir(&base_dir)?;
    let other_wdk_sys_out_dir = extract_out_dir(&other_dir)?;

    // collect all .rs files in OUT_DIR of other into hashset of paths
    let mut other_generated_rs_filepaths = WalkBuilder::new(&other_wdk_sys_out_dir)
        .standard_filters(false)
        .build()
        // filter for only .rs files (errors are forwarded)
        .filter_map(|dir_entry| match dir_entry {
            Err(err) => return Some(Err(err)),
            Ok(dir_entry) => {
                if dir_entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_file())
                    && dir_entry.path().extension().is_some_and(|ext| ext == "rs")
                {
                    return Some(Ok(dir_entry.path().to_owned()));
                }
                return None;
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .collect::<HashSet<_>>();

    // iterate all files in base_dir
    for base_generated_rs_filepath in WalkBuilder::new(&base_wdk_sys_out_dir)
        .standard_filters(false)
        .build()
        .filter_map(|dir_entry| match dir_entry {
            Err(err) => Some(Err(err)),
            Ok(dir_entry) => {
                if dir_entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_file())
                    && dir_entry.path().extension().is_some_and(|ext| ext == "rs")
                {
                    return Some(Ok(dir_entry.path().to_owned()));
                }
                None
            }
        })
    {
        let base_generated_rs_filepath = base_generated_rs_filepath?;
        let relative_filepath =
            base_generated_rs_filepath.strip_prefix(base_wdk_sys_out_dir.as_path())?;
        let other_generated_rs_filepath = other_wdk_sys_out_dir.join_os(relative_filepath);

        generate_diff(
            Some(&base_generated_rs_filepath),
            other_generated_rs_filepaths
                .take(&other_generated_rs_filepath)
                .as_deref(),
        )?;
    }

    // file is missing in base. Diff blank with other
    for path in other_generated_rs_filepaths {
        generate_diff(None, Some(&path))?;
    }

    Ok(())
}

// TODO: configurable wdk configuration. use Serde?
#[tracing::instrument(level = "trace")]
fn inject_wdk_configuration(base_dir: &PathBuf) -> Result<()> {
    let workspace_cargo_manifest_path = base_dir
        .join("Cargo.toml")
        .canonicalize()?
        .strip_extended_length_path_prefix()?;
    let mut workspace_cargo_manifest_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&workspace_cargo_manifest_path)?;
    workspace_cargo_manifest_file.write_all(
        r#"
# Injected by wdk-bindings-diff
[workspace.metadata.wdk.driver-model]
driver-type = "KMDF"
kmdf-version-major = 1
target-kmdf-version-minor = 33
"#
        .as_bytes(),
    )?;

    Ok(())
}

#[tracing::instrument(level = "trace")]
fn extract_out_dir(repo_root: &Path) -> anyhow::Result<Utf8PathBuf> {
    let manifest_path = repo_root
        .join("Cargo.toml")
        .strip_extended_length_path_prefix()?;

    // find wdk-sys pkg_id
    let metadata_command = {
        let mut metadata_command = MetadataCommand::new();
        metadata_command.manifest_path(&manifest_path); // TODO: features?
        metadata_command
    };
    let cargo = metadata_command
        .cargo_command()
        .get_program()
        .to_os_string();

    let metadata = metadata_command.exec()?;
    let wdk_sys_pkg_id = &metadata
        .packages
        .iter()
        .find(|package| package.name == "wdk-sys")
        .ok_or(anyhow!(
            "wdk-sys package should exist in cargo metadata output"
        ))?
        .id;

    // parse cargo check output to extract OUT_DIR
    let args = [
        "check".as_ref(),
        "--manifest-path".as_ref(),
        manifest_path.as_os_str(),
        "--message-format=json-render-diagnostics".as_ref(),
        "--package".as_ref(),
        "wdk-sys".as_ref(),
    ];
    let mut command = Command::new(cargo)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let reader = std::io::BufReader::new(
        command
            .stdout
            .take()
            .expect("stdout should be captured and its handle should be valid"),
    );
    for message in cargo_metadata::Message::parse_stream(reader) {
        if let Message::BuildScriptExecuted(build_script_json_output) = message? {
            if build_script_json_output.package_id == *wdk_sys_pkg_id {
                return Ok(build_script_json_output.out_dir);
            }
        }
    }
    command.wait()?.success().then_some(()).ok_or_else(|| {
        let mut stderr_output = String::new();

        if let Err(err) = command
            .stderr
            .expect("stderr should be captured and its handle should be valid")
            .read_to_string(&mut stderr_output)
        {
            stderr_output.push_str(&format!("\nfailed to read stderr to end: {:#?}", err));
        }

        anyhow!("cargo {:#?} failed", args.join(" ".as_ref())).context(stderr_output)
    })?;

    bail!("failed to extract OUT_DIR from wdk-sys build");
}

#[tracing::instrument(level = "trace")]
fn generate_diff(base_path: Option<&Path>, other_path: Option<&Path>) -> anyhow::Result<()> {
    let base_file_contents = base_path
        .map(|path| std::fs::read_to_string(path))
        .transpose()?
        .unwrap_or_default();
    let other_file_contents = other_path
        .map(|path| std::fs::read_to_string(path))
        .transpose()?
        .unwrap_or_default();

    let diff = similar::TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_lines(&base_file_contents, &other_file_contents);

    // TODO: handle empty path as what the path WOULD have beenisntead of empty
    println!(
        "--- {}",
        base_path
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!("+++ {}", base_path.map(|path| path.display().to_string()).unwrap_or_default());

    for (change_cluster_index, change_cluster) in diff.grouped_ops(3).into_iter().enumerate() {
        if change_cluster_index > 0 {
            println!("{:-^width$}", "", width = 80);
        }
        for diff_change in change_cluster {
            for inline_change in diff.iter_inline_changes(&diff_change) {
                // no need for this mapping since diplay is implemented on changetag already
                let (sign, style) = match inline_change.tag() {
                    ChangeTag::Delete => ("-", Style::new().red()),
                    ChangeTag::Insert => ("+", Style::new().green()),
                    ChangeTag::Equal => (" ", Style::new().dim()),
                };

                // TODO: clean this up... maybe make this resolve instead of a struct like this
                struct Line(Option<usize>);
                impl fmt::Display for Line {
                    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        match self.0 {
                            None => write!(f, "    "),
                            Some(idx) => write!(f, "{:<4}", idx + 1),
                        }
                    }
                }

                // Prints line numbers + | + diff change type
                print!(
                    "{}{} |{}",
                    console::style(Line(inline_change.old_index())).dim(),
                    console::style(Line(inline_change.new_index())).dim(),
                    style.apply_to(sign).bold(),
                );
                for (emphasized, value) in inline_change.iter_strings_lossy() {
                    if emphasized {
                        print!("{}", style.apply_to(value).underlined().on_black());
                    } else {
                        print!("{}", style.apply_to(value));
                    }
                }
                if inline_change.missing_newline() {
                    println!();
                }
            }
        }
    }

    // TODO: this function should return a writer or string, and then caller should
    // handler formatting etc
    Ok(())
}
