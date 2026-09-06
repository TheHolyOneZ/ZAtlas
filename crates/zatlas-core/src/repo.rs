use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::config::ZatlasConfig;
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RepoInfo {
    pub root: String,
    pub name: String,

    pub branch: Option<String>,

    pub head: Option<String>,
    pub is_git: bool,

    pub has_config: bool,

    pub invalid_rules: Vec<String>,
}

fn simplify(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

pub fn open(root: &Path) -> Result<RepoInfo> {
    if !root.is_dir() {
        return Err(CoreError::io(
            root,
            std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory"),
        ));
    }

    let root = simplify(root.canonicalize().map_err(|e| CoreError::io(root, e))?);

    let config = ZatlasConfig::load(&root)?;
    let has_config = root.join(ZatlasConfig::FILE_NAME).is_file();

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());

    let (is_git, branch, head) = match git_head(&root) {
        Some((branch, head)) => (true, branch, head),
        None => (false, None, None),
    };

    Ok(RepoInfo {
        root: root.to_string_lossy().into_owned(),
        name,
        branch,
        head,
        is_git,
        has_config,
        invalid_rules: config.invalid_rules(),
    })
}

fn git_head(root: &Path) -> Option<(Option<String>, Option<String>)> {
    let repo = gix::open(root).ok()?;

    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string());

    let head = repo
        .head_id()
        .ok()
        .map(|id| id.to_hex_with_len(7).to_string());

    Some((branch, head))
}

pub fn ensure_cache_dir(root: &Path) -> Result<PathBuf> {
    let dir = crate::config::cache_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::io(&dir, e))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git should be available in the test environment")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn a_verbatim_windows_path_is_unwrapped_for_display() {
        assert_eq!(
            simplify(PathBuf::from(r"\\?\C:\code\acme")),
            PathBuf::from(r"C:\code\acme")
        );

        let unc = PathBuf::from(r"\\?\UNC\server\share");
        assert_eq!(simplify(unc.clone()), unc);

        assert_eq!(
            simplify(PathBuf::from("/home/me/x")),
            PathBuf::from("/home/me/x")
        );
    }

    #[test]
    fn a_plain_folder_opens_without_git_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let info = open(dir.path()).unwrap();
        assert!(!info.is_git);
        assert_eq!(info.branch, None);
        assert_eq!(info.head, None);
        assert!(!info.has_config);
    }

    #[test]
    fn opening_something_that_is_not_a_directory_is_an_error_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let err = open(&file).unwrap_err();
        assert!(err.to_string().contains("a.txt"), "got: {err}");
    }

    #[test]
    fn a_git_repository_reports_its_branch_and_head() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        git(p, &["config", "user.email", "t@example.com"]);
        git(p, &["config", "user.name", "Test"]);
        std::fs::write(p.join("a.txt"), "hello").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-qm", "first"]);

        let info = open(p).unwrap();
        assert!(info.is_git);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert_eq!(info.head.as_ref().map(|h| h.len()), Some(7));
    }

    #[test]
    fn a_git_repository_with_no_commits_is_still_a_git_repository() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);

        let info = open(dir.path()).unwrap();
        assert!(info.is_git);
        assert_eq!(info.head, None);
    }

    #[test]
    fn a_config_with_a_typo_in_a_rule_surfaces_it_on_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("zatlas.toml"),
            "[[layers]]\nname=\"ui\"\npaths=[]\nrules=[\"ui -> domian\"]",
        )
        .unwrap();
        let info = open(dir.path()).unwrap();
        assert!(info.has_config);
        assert_eq!(info.invalid_rules, ["ui -> domian"]);
    }

    #[test]
    fn the_cache_directory_is_created_inside_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ensure_cache_dir(dir.path()).unwrap();
        assert!(cache.is_dir());
        assert_eq!(cache.file_name().unwrap(), ".zatlas");
    }
}
