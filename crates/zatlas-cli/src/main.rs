mod commands;
mod load;
mod style;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use load::LoadOptions;
use style::Style;

const EXIT_THRESHOLD: i32 = 1;
const EXIT_ERROR: i32 = 2;

#[derive(Parser)]
#[command(
    name = "zatlas",
    version,
    about = "Draw the shape of a codebase: dependencies, cycles, hotspots and risk.",
    long_about = "ZAtlas reads a repository and reports its structure. It never modifies \
                  your source or your history; the only thing it writes is its own cache \
                  under <repo>/.zatlas/.\n\n\
                  Everything runs locally. Nothing is uploaded, ever."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long, global = true)]
    no_cache: bool,

    #[arg(long, global = true)]
    no_git: bool,
}

#[derive(Subcommand)]
enum Command {
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Findings {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long, value_enum)]
        kind: Vec<KindArg>,

        #[arg(long, value_enum)]
        severity: Option<SeverityArg>,

        #[arg(long, default_value_t = 40)]
        limit: usize,

        #[arg(long)]
        explain: bool,

        #[arg(long, value_enum)]
        fail_on: Option<SeverityArg>,

        #[arg(long)]
        max_unresolved: Option<usize>,

        #[arg(long)]
        all: bool,
    },

    Cycles {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long)]
        fail: bool,
    },

    Unresolved {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long)]
        all: bool,
    },

    Impact {
        #[arg(required = true)]
        files: Vec<String>,
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },

    Tour {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = 10)]
        stops: usize,
    },

    Compare {
        base: String,

        #[arg(default_value = "HEAD")]
        head: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,

        #[arg(long)]
        fail_on_cycle: bool,
    },

    Baseline {
        #[command(subcommand)]
        action: BaselineAction,
    },

    Export {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = FormatArg::D2)]
        format: FormatArg,
        #[arg(long, value_enum, default_value_t = ScopeArg::Modules)]
        scope: ScopeArg,

        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum BaselineAction {
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Accept {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long, value_enum)]
        below: Option<SeverityArg>,
    },

    Drop {
        #[arg(required = true)]
        pattern: String,
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    Prune {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum SeverityArg {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl From<SeverityArg> for zatlas_core::findings::Severity {
    fn from(value: SeverityArg) -> Self {
        use zatlas_core::findings::Severity as S;
        match value {
            SeverityArg::Info => S::Info,
            SeverityArg::Low => S::Low,
            SeverityArg::Medium => S::Medium,
            SeverityArg::High => S::High,
            SeverityArg::Critical => S::Critical,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    Cycle,
    GodFile,
    Orphan,
    UnstableInterface,
    LayeringViolation,
    BusFactor,
    DistantCoupling,
    BarrelHub,
    CaseMismatch,
    LspDisagreement,
    PathAlias,
}

impl From<KindArg> for zatlas_core::findings::FindingKind {
    fn from(value: KindArg) -> Self {
        use zatlas_core::findings::FindingKind as K;
        match value {
            KindArg::Cycle => K::Cycle,
            KindArg::GodFile => K::GodFile,
            KindArg::Orphan => K::Orphan,
            KindArg::UnstableInterface => K::UnstableInterface,
            KindArg::LayeringViolation => K::LayeringViolation,
            KindArg::BusFactor => K::BusFactor,
            KindArg::DistantCoupling => K::DistantCoupling,
            KindArg::BarrelHub => K::BarrelHub,
            KindArg::CaseMismatch => K::CaseMismatch,
            KindArg::LspDisagreement => K::LspDisagreement,
            KindArg::PathAlias => K::PathAlias,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    D2,
    Mermaid,
    Markdown,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    Modules,
    Files,
}

impl From<ScopeArg> for zatlas_core::export::diagram::DiagramScope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Modules => Self::Modules,
            ScopeArg::Files => Self::Files,
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let style = if cli.json {
        Style::plain()
    } else {
        Style::of_stdout()
    };
    let opts = LoadOptions {
        use_cache: !cli.no_cache,
        git: !cli.no_git,
    };

    let result = match cli.command {
        Command::Scan { path } => commands::scan(&path, &opts, cli.json, style),
        Command::Findings {
            path,
            kind,
            severity,
            limit,
            explain,
            fail_on,
            max_unresolved,
            all,
        } => commands::findings(
            &path,
            &opts,
            cli.json,
            style,
            commands::FindingsArgs {
                kinds: kind.into_iter().map(Into::into).collect(),
                severity: severity.map(Into::into),
                limit,
                explain,
                fail_on: fail_on.map(Into::into),
                max_unresolved,
                include_accepted: all,
            },
        ),
        Command::Cycles { path, fail } => commands::cycles(&path, &opts, cli.json, style, fail),
        Command::Unresolved { path, all } => {
            commands::unresolved(&path, &opts, cli.json, style, all)
        }
        Command::Impact { files, path } => commands::impact(&path, &opts, cli.json, style, &files),
        Command::Tour { path, stops } => commands::tour(&path, &opts, cli.json, style, stops),
        Command::Compare {
            base,
            head,
            path,
            fail_on_cycle,
        } => commands::compare(&path, cli.json, style, &base, &head, fail_on_cycle),
        Command::Baseline { action } => match action {
            BaselineAction::Status { path } => {
                commands::baseline_status(&path, &opts, cli.json, style)
            }
            BaselineAction::Accept { path, below } => {
                commands::baseline_accept(&path, &opts, style, below.map(Into::into))
            }
            BaselineAction::Drop { pattern, path } => {
                commands::baseline_drop(&path, &opts, style, &pattern)
            }
            BaselineAction::Prune { path } => commands::baseline_prune(&path, &opts, style),
        },
        Command::Export {
            path,
            format,
            scope,
            out,
        } => commands::export(&path, &opts, format, scope, out.as_deref()),
    };

    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(EXIT_THRESHOLD),
        Err(message) => {
            eprintln!(
                "{}: {message}",
                style.severity(zatlas_core::findings::Severity::High, "zatlas",)
            );
            std::process::exit(EXIT_ERROR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_argument_parser_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn a_path_defaults_to_the_current_directory() {
        let cli = Cli::parse_from(["zatlas", "scan"]);
        match cli.command {
            Command::Scan { path } => assert_eq!(path, PathBuf::from(".")),
            _ => panic!("wrong subcommand"),
        }
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand_too() {
        let cli = Cli::parse_from(["zatlas", "findings", ".", "--json"]);
        assert!(cli.json);
    }

    #[test]
    fn fail_on_parses_the_severity_names_the_app_uses() {
        let cli = Cli::parse_from(["zatlas", "findings", "--fail-on", "high"]);
        match cli.command {
            Command::Findings { fail_on, .. } => {
                assert_eq!(
                    fail_on.map(zatlas_core::findings::Severity::from),
                    Some(zatlas_core::findings::Severity::High)
                );
            }
            _ => panic!("wrong subcommand"),
        }
    }

    #[test]
    fn impact_requires_at_least_one_file_rather_than_silently_doing_nothing() {
        assert!(Cli::try_parse_from(["zatlas", "impact"]).is_err());
    }

    #[test]
    fn every_finding_kind_is_addressable_from_the_command_line() {
        for kind in [
            KindArg::Cycle,
            KindArg::GodFile,
            KindArg::Orphan,
            KindArg::UnstableInterface,
            KindArg::LayeringViolation,
            KindArg::BusFactor,
            KindArg::DistantCoupling,
            KindArg::BarrelHub,
            KindArg::CaseMismatch,
            KindArg::LspDisagreement,
            KindArg::PathAlias,
        ] {
            let _: zatlas_core::findings::FindingKind = kind.into();
        }
    }
}
