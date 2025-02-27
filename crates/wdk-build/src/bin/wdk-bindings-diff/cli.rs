// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

use std::fmt::{self, Display};

use clap::{
    builder::{NonEmptyStringValueParser, TryMapValueParser, TypedValueParser, ValueParserFactory},
    value_parser,
};
use clap_cargo::style::CLAP_STYLING;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None, styles = CLAP_STYLING)]
pub struct CommandLineInterface {
    /// The base to diff against
    #[arg(value_parser = value_parser!(DiffBase), default_value_t = DiffBase::LatestMain)]
    pub(crate) diff_base: DiffBase,

    /// The target to diff against
    #[arg(value_parser = value_parser!(DiffTarget), default_value_t = DiffTarget::Local)]
    pub(crate) diff_target: DiffTarget,

    #[command(flatten)]
    pub(crate) verbose: clap_verbosity_flag::Verbosity,

    #[command(flatten)]
    pub(crate) color: colorchoice_clap::Color,

    #[command(flatten)]
    #[command(next_help_heading = "Package Selection")]
    pub(crate) workspace: clap_cargo::Workspace,

    #[command(flatten)]
    #[command(next_help_heading = "Feature Selection")]
    pub(crate) features: clap_cargo::Features,
    // FIXME: long_help messages have unintended newline: https://github.com/clap-rs/clap/issues/5915

    // #[command(flatten)]
    // compilation_options: CompilationOptions,

    // #[command(flatten)]
    // manifest_options: clap_cargo::Manifest,
}

#[derive(Clone, Debug)]
pub enum DiffBase {
    LatestMain,
    GitRev(String),
}

#[derive(Clone, Debug)]
pub enum DiffTarget {
    Local,
    GitRev(String),
}

const DIFF_BASE_LATEST_MAIN_DISPLAY_STRING: &str = "latest-main";
const DIFF_TARGET_LOCAL_DISPLAY_STRING: &str = "local";

impl Display for DiffBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LatestMain => write!(f, "{DIFF_BASE_LATEST_MAIN_DISPLAY_STRING}"),
            Self::GitRev(git_hash) => write!(f, "Git Rev({git_hash})"),
        }
    }
}

impl ValueParserFactory for DiffBase {
    type Parser =
        TryMapValueParser<NonEmptyStringValueParser, fn(String) -> Result<Self, git2::Error>>;

    fn value_parser() -> Self::Parser {
        let parser = NonEmptyStringValueParser::new();
        parser.try_map(|s| {
            if s.eq_ignore_ascii_case(DIFF_BASE_LATEST_MAIN_DISPLAY_STRING) {
                Ok(Self::LatestMain)
            } else {
                Ok(Self::GitRev(s))
            }
        })
    }
}

impl Display for DiffTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "{DIFF_TARGET_LOCAL_DISPLAY_STRING}"),
            Self::GitRev(git_hash) => write!(f, "Git Rev({git_hash})"),
        }
    }
}

// FIXME: validate git rev based on repo arg
impl ValueParserFactory for DiffTarget {
    type Parser =
        TryMapValueParser<NonEmptyStringValueParser, fn(String) -> Result<Self, git2::Error>>;

    fn value_parser() -> Self::Parser {
        let parser = NonEmptyStringValueParser::new();
        parser.try_map(|s| {
            if s.eq_ignore_ascii_case(DIFF_TARGET_LOCAL_DISPLAY_STRING) {
                Ok(Self::Local)
            } else {
                Ok(Self::GitRev(s))
            }
        })
    }
}
