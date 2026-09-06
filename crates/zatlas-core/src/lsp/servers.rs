use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::Language;

pub struct ServerSpec {
    pub id: &'static str,
    pub label: &'static str,

    pub command: &'static str,
    pub args: &'static [&'static str],
    pub languages: &'static [Language],

    pub root_markers: &'static [&'static str],
}

impl ServerSpec {
    pub fn handles(&self, language: Language) -> bool {
        self.languages.contains(&language)
    }
}

pub fn language_id(language: Language) -> &'static str {
    match language {
        Language::TypeScript => "typescript",
        Language::Tsx => "typescriptreact",
        Language::JavaScript => "javascript",
        Language::Jsx => "javascriptreact",
        Language::Rust => "rust",
        Language::Python => "python",
        Language::Go => "go",
    }
}

const TS: &[Language] = &[
    Language::TypeScript,
    Language::Tsx,
    Language::JavaScript,
    Language::Jsx,
];

pub static KNOWN: &[ServerSpec] = &[
    ServerSpec {
        id: "rust-analyzer",
        label: "rust-analyzer",
        command: "rust-analyzer",
        args: &[],
        languages: &[Language::Rust],
        root_markers: &["Cargo.toml"],
    },
    ServerSpec {
        id: "typescript-language-server",
        label: "TypeScript Language Server",
        command: "typescript-language-server",
        args: &["--stdio"],
        languages: TS,
        root_markers: &["package.json", "tsconfig.json", "jsconfig.json"],
    },
    ServerSpec {
        id: "gopls",
        label: "gopls",
        command: "gopls",
        args: &[],
        languages: &[Language::Go],
        root_markers: &["go.mod"],
    },
    ServerSpec {
        id: "pyright",
        label: "Pyright",
        command: "pyright-langserver",
        args: &["--stdio"],
        languages: &[Language::Python],
        root_markers: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
        ],
    },
    ServerSpec {
        id: "basedpyright",
        label: "basedpyright",
        command: "basedpyright-langserver",
        args: &["--stdio"],
        languages: &[Language::Python],
        root_markers: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
        ],
    },
    ServerSpec {
        id: "pylsp",
        label: "python-lsp-server",
        command: "pylsp",
        args: &[],
        languages: &[Language::Python],
        root_markers: &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
        ],
    },
];

pub struct Discovered {
    pub spec: &'static ServerSpec,
    pub executable: PathBuf,
}

impl Discovered {
    pub fn command(&self) -> Command {
        let is_shim = self
            .executable
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));

        if cfg!(windows) && is_shim {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&self.executable).args(self.spec.args);
            c
        } else {
            let mut c = Command::new(&self.executable);
            c.args(self.spec.args);
            c
        }
    }
}

pub fn discover(root: &Path, languages: &[Language], wanted: &[String]) -> Vec<Discovered> {
    let mut out: Vec<Discovered> = Vec::new();
    for spec in KNOWN {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == spec.id) {
            continue;
        }
        if !spec.languages.iter().any(|l| languages.contains(l)) {
            continue;
        }
        if !spec.root_markers.is_empty() && !spec.root_markers.iter().any(|m| root.join(m).exists())
        {
            continue;
        }

        if out
            .iter()
            .any(|d| d.spec.languages.iter().any(|l| spec.handles(*l)))
        {
            continue;
        }
        if let Some(executable) = find_on_path(spec.command) {
            out.push(Discovered { spec, executable });
        }
    }
    out
}

pub fn find_on_path(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        Vec::new()
    };

    for dir in std::env::split_paths(&path) {
        if extensions.is_empty() {
            let base = dir.join(command);
            if is_executable(&base) {
                return Some(base);
            }
        }
        for extension in &extensions {
            let with_ext = dir.join(format!("{command}{extension}"));
            if is_executable(&with_ext) {
                return Some(with_ext);
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_server_has_a_language_id_for_each_language_it_claims() {
        for spec in KNOWN {
            assert!(!spec.languages.is_empty(), "{} handles nothing", spec.id);
            for language in spec.languages {
                assert!(!language_id(*language).is_empty());
            }
        }
    }

    #[test]
    fn server_ids_are_unique_because_config_addresses_them_by_id() {
        let mut ids: Vec<&str> = KNOWN.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn a_repository_without_the_marker_file_starts_nothing() {
        let dir = tempfile::tempdir().unwrap();

        let found = discover(dir.path(), &[Language::Rust], &[]);
        assert!(found.is_empty());
    }

    #[test]
    fn a_language_the_scan_did_not_find_never_starts_a_server() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert!(discover(dir.path(), &[Language::Python], &[]).is_empty());
    }

    #[test]
    fn the_allowlist_excludes_servers_not_named_in_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let wanted = vec!["gopls".to_owned()];
        assert!(discover(dir.path(), &[Language::Rust], &wanted).is_empty());
    }

    #[test]
    fn on_windows_only_pathext_variants_are_candidates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pretend-server"), "not a program").unwrap();
        let restore = std::env::var_os("PATH");
        std::env::set_var("PATH", dir.path());
        let found = find_on_path("pretend-server");
        if let Some(old) = restore {
            std::env::set_var("PATH", old);
        }
        if cfg!(windows) {
            assert!(found.is_none(), "an extensionless file is not runnable");
        } else {
            assert!(found.is_none());
        }
    }

    #[test]
    fn find_on_path_locates_a_real_program() {
        let name = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(find_on_path(name).is_some(), "{name} should be on PATH");
        assert!(find_on_path("zatlas-definitely-not-a-real-program").is_none());
    }

    #[test]
    fn a_directory_on_path_is_not_mistaken_for_an_executable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("notaprogram")).unwrap();
        assert!(!is_executable(&dir.path().join("notaprogram")));
    }
}
