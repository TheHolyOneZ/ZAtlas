use std::collections::HashMap;
use std::path::Path;

use super::FileIndex;
use crate::model::{AliasAudit, FileId, UnresolvedReason};

const EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".mts", ".cts", ".d.ts", ".js", ".jsx", ".mjs", ".cjs",
];

const ASSET_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp",
    ".avif", ".ico", ".bmp", ".json", ".json5", ".yaml", ".yml", ".toml", ".txt", ".md", ".mdx",
    ".wasm", ".woff", ".woff2", ".ttf", ".otf", ".eot", ".mp3", ".mp4", ".webm", ".ogg", ".wav",
    ".glsl", ".frag", ".vert", ".graphql", ".gql", ".html", ".xml", ".csv", ".pdf",
];

pub fn asset_extension(specifier: &str) -> Option<&'static str> {
    let base = specifier
        .split(['?', '#'])
        .next()
        .unwrap_or(specifier)
        .to_ascii_lowercase();
    ASSET_EXTENSIONS
        .iter()
        .find(|ext| base.ends_with(*ext))
        .copied()
}

const INDEX_FILES: &[&str] = &[
    "index.ts",
    "index.tsx",
    "index.mts",
    "index.cts",
    "index.d.ts",
    "index.js",
    "index.jsx",
    "index.mjs",
    "index.cjs",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAlias {
    pub prefix: String,
    pub suffix: String,
    pub has_wildcard: bool,

    pub targets: Vec<String>,

    pub source: String,

    pub pattern: String,
}

#[derive(Debug, Clone, Default)]
pub struct JsProject {
    pub aliases: Vec<PathAlias>,

    pub base_urls: Vec<String>,

    pub workspace_packages: HashMap<String, String>,
}

mod jsonc {

    pub fn strip_comments(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let bytes = input.as_bytes();
        let mut i = 0;
        let mut in_string = false;
        let mut escaped = false;

        while i < bytes.len() {
            let c = bytes[i];
            if in_string {
                out.push(c as char);
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            if c == b'"' {
                in_string = true;
                out.push('"');
                i += 1;
                continue;
            }
            if c == b'/' && i + 1 < bytes.len() {
                if bytes[i + 1] == b'/' {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                if bytes[i + 1] == b'*' {
                    i += 2;
                    while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    i += 2;
                    continue;
                }
            }
            out.push(c as char);
            i += 1;
        }
        out
    }

    pub fn string_field(src: &str, key: &str, from: usize) -> Option<String> {
        let needle = format!("\"{key}\"");
        let at = src.get(from..)?.find(&needle)? + from + needle.len();
        let rest = src.get(at..)?;
        let colon = rest.find(':')? + 1;
        let after = rest.get(colon..)?;
        let open = after.find('"')?;
        let tail = after.get(open + 1..)?;
        let close = tail.find('"')?;
        Some(tail[..close].to_string())
    }

    pub fn find_key(src: &str, key: &str) -> Option<usize> {
        let needle = format!("\"{key}\"");
        src.find(&needle).map(|i| i + needle.len())
    }

    pub fn string_array(src: &str, at: usize) -> Vec<String> {
        let Some(rest) = src.get(at..) else {
            return Vec::new();
        };
        let Some(open) = rest.find('[') else {
            return Vec::new();
        };
        let Some(close) = rest[open..].find(']') else {
            return Vec::new();
        };
        let body = &rest[open + 1..open + close];
        let mut out = Vec::new();
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut s = String::new();
            for c2 in chars.by_ref() {
                if c2 == '"' {
                    break;
                }
                s.push(c2);
            }
            if !s.is_empty() {
                out.push(s);
            }
        }
        out
    }
}

pub fn join_rel(base_dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !base_dir.is_empty() {
        parts.extend(base_dir.split('/').filter(|s| !s.is_empty()));
    }
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

pub fn build_project(root: &Path, index: &FileIndex, all_files: &[String]) -> JsProject {
    let mut project = JsProject::default();

    for rel in all_files {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let dir = parent_dir(rel).to_string();

        if name == "tsconfig.json" || name.starts_with("tsconfig.") && name.ends_with(".json") {
            let Ok(raw) = std::fs::read_to_string(crate::paths::rel_to_path(root, rel)) else {
                continue;
            };
            let src = jsonc::strip_comments(&raw);
            let base_url = jsonc::string_field(&src, "baseUrl", 0).unwrap_or_default();
            let base_dir = if base_url.is_empty() {
                dir.clone()
            } else {
                join_rel(&dir, &base_url)
            };
            if !base_url.is_empty() {
                project.base_urls.push(base_dir.clone());
            }
            collect_aliases(&src, &base_dir, rel, &mut project.aliases);
        }

        if name == "package.json" {
            let Ok(raw) = std::fs::read_to_string(crate::paths::rel_to_path(root, rel)) else {
                continue;
            };
            let src = jsonc::strip_comments(&raw);
            if let Some(pkg_name) = jsonc::string_field(&src, "name", 0) {
                if !dir.is_empty() {
                    project.workspace_packages.insert(pkg_name, dir.clone());
                }
            }
        }
    }

    let _ = index;

    project
        .aliases
        .sort_by_key(|a| std::cmp::Reverse(a.prefix.len()));
    project.base_urls.sort();
    project.base_urls.dedup();
    project
}

fn collect_aliases(src: &str, base_dir: &str, source: &str, out: &mut Vec<PathAlias>) {
    let Some(paths_at) = jsonc::find_key(src, "paths") else {
        return;
    };
    let Some(rest) = src.get(paths_at..) else {
        return;
    };
    let Some(open) = rest.find('{') else {
        return;
    };

    let body = &rest[open + 1..];
    let mut depth = 0i32;
    let mut i = 0usize;
    let bytes = body.as_bytes();
    let mut end = body.len();
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    end = i;
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    let body = &body[..end];

    let mut cursor = 0usize;
    while let Some(qs) = body[cursor..].find('"') {
        let key_start = cursor + qs + 1;
        let Some(qe) = body[key_start..].find('"') else {
            break;
        };
        let key = &body[key_start..key_start + qe];
        let after_key = key_start + qe + 1;
        let targets = jsonc::string_array(body, after_key);
        if targets.is_empty() {
            cursor = after_key;
            continue;
        }

        let (prefix, suffix, has_wildcard) = match key.split_once('*') {
            Some((p, s)) => (p.to_string(), s.to_string(), true),
            None => (key.to_string(), String::new(), false),
        };

        out.push(PathAlias {
            prefix,
            suffix,
            has_wildcard,
            targets: targets
                .into_iter()
                .map(|t| join_rel(base_dir, &t))
                .collect(),
            source: source.to_string(),
            pattern: key.to_string(),
        });

        let arr_end = body[after_key..]
            .find(']')
            .map(|i| after_key + i + 1)
            .unwrap_or(body.len());
        cursor = arr_end;
    }
}

fn try_candidate_cased(candidate: &str, index: &FileIndex) -> Option<(FileId, bool)> {
    if let Some(id) = index.id(candidate) {
        return Some((id, true));
    }
    for ext in EXTENSIONS {
        if let Some(id) = index.id(&format!("{candidate}{ext}")) {
            return Some((id, true));
        }
    }
    for idx in INDEX_FILES {
        let joined = index_path(candidate, idx);
        if let Some(id) = index.id(&joined) {
            return Some((id, true));
        }
    }

    if let Some(hit) = index.id_case_insensitive(candidate) {
        return Some(hit);
    }
    for ext in EXTENSIONS {
        if let Some(hit) = index.id_case_insensitive(&format!("{candidate}{ext}")) {
            return Some(hit);
        }
    }
    for idx in INDEX_FILES {
        if let Some(hit) = index.id_case_insensitive(&index_path(candidate, idx)) {
            return Some(hit);
        }
    }
    None
}

fn index_path(candidate: &str, index_file: &str) -> String {
    if candidate.is_empty() {
        index_file.to_string()
    } else {
        format!("{candidate}/{index_file}")
    }
}

pub fn candidate_paths(from_path: &str, specifier: &str, project: &JsProject) -> Vec<String> {
    let mut out = Vec::new();
    if specifier.is_empty() {
        return out;
    }

    if specifier.starts_with("./") || specifier.starts_with("../") {
        out.push(join_rel(parent_dir(from_path), specifier));
        return out;
    }
    if let Some(stripped) = specifier.strip_prefix('/') {
        out.push(stripped.to_string());
        return out;
    }

    for alias in &project.aliases {
        let matched = if alias.has_wildcard {
            specifier.starts_with(&alias.prefix) && specifier.ends_with(&alias.suffix)
        } else {
            specifier == alias.prefix
        };
        if !matched {
            continue;
        }
        let star = if alias.has_wildcard {
            let start = alias.prefix.len();
            let end = specifier.len() - alias.suffix.len();
            if start > end {
                continue;
            }
            &specifier[start..end]
        } else {
            ""
        };
        for target in &alias.targets {
            out.push(if alias.has_wildcard {
                target.replacen('*', star, 1)
            } else {
                target.clone()
            });
        }
    }

    for (pkg, dir) in &project.workspace_packages {
        if specifier == pkg {
            out.push(dir.clone());
        } else if let Some(sub) = specifier.strip_prefix(&format!("{pkg}/")) {
            out.push(join_rel(dir, sub));
        }
    }

    for base in &project.base_urls {
        out.push(join_rel(base, specifier));
    }

    out
}

pub fn resolve_specifier(
    from_path: &str,
    specifier: &str,
    index: &FileIndex,
    project: &JsProject,
) -> Result<FileId, UnresolvedReason> {
    resolve_specifier_cased(from_path, specifier, index, project).map(|(id, _)| id)
}

pub fn resolve_specifier_cased(
    from_path: &str,
    specifier: &str,
    index: &FileIndex,
    project: &JsProject,
) -> Result<(FileId, bool), UnresolvedReason> {
    if specifier.is_empty() {
        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    if let Some(ext) = asset_extension(specifier) {
        return Err(UnresolvedReason::Asset(
            ext.trim_start_matches('.').to_string(),
        ));
    }

    if specifier.starts_with("./")
        || specifier.starts_with("../")
        || specifier == "."
        || specifier == ".."
    {
        let dir = parent_dir(from_path);
        let candidate = join_rel(dir, specifier);
        return try_candidate_cased(&candidate, index)
            .ok_or_else(|| UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    if let Some(stripped) = specifier.strip_prefix('/') {
        return try_candidate_cased(stripped, index)
            .ok_or_else(|| UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    for alias in &project.aliases {
        let matched = if alias.has_wildcard {
            specifier.starts_with(&alias.prefix) && specifier.ends_with(&alias.suffix)
        } else {
            specifier == alias.prefix
        };
        if !matched {
            continue;
        }

        let star = if alias.has_wildcard {
            let start = alias.prefix.len();
            let end = specifier.len() - alias.suffix.len();
            if start > end {
                continue;
            }
            &specifier[start..end]
        } else {
            ""
        };

        for target in &alias.targets {
            let candidate = if alias.has_wildcard {
                target.replacen('*', star, 1)
            } else {
                target.clone()
            };
            if let Some(hit) = try_candidate_cased(&candidate, index) {
                return Ok(hit);
            }
        }

        return Err(UnresolvedReason::UnmatchedAlias(specifier.to_string()));
    }

    for (pkg, dir) in &project.workspace_packages {
        if specifier == pkg {
            if let Some(hit) = try_candidate_cased(dir, index) {
                return Ok(hit);
            }
        } else if let Some(sub) = specifier.strip_prefix(&format!("{pkg}/")) {
            let candidate = join_rel(dir, sub);
            if let Some(hit) = try_candidate_cased(&candidate, index) {
                return Ok(hit);
            }
        }
    }

    for base in &project.base_urls {
        let candidate = join_rel(base, specifier);
        if let Some(hit) = try_candidate_cased(&candidate, index) {
            return Ok(hit);
        }
    }

    let package = if let Some(rest) = specifier.strip_prefix('@') {
        rest.split('/')
            .take(2)
            .fold(String::from("@"), |mut acc, part| {
                if acc.len() > 1 {
                    acc.push('/');
                }
                acc.push_str(part);
                acc
            })
    } else {
        specifier.split('/').next().unwrap_or(specifier).to_string()
    };
    Err(UnresolvedReason::External(package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;

    fn index(paths: &[&str]) -> FileIndex {
        let paths: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let langs = vec![Language::TypeScript; paths.len()];
        FileIndex::new(paths, langs)
    }

    fn resolve(from: &str, spec: &str, idx: &FileIndex) -> Result<FileId, UnresolvedReason> {
        resolve_specifier(from, spec, idx, &JsProject::default())
    }

    #[test]
    fn join_rel_normalises_dot_and_dotdot() {
        assert_eq!(join_rel("src/ui", "./a"), "src/ui/a");
        assert_eq!(join_rel("src/ui", "../domain/b"), "src/domain/b");
        assert_eq!(join_rel("src/a/b", "../../c"), "src/c");
        assert_eq!(join_rel("", "./a"), "a");
        assert_eq!(join_rel("src", "../../escape"), "escape");
    }

    #[test]
    fn a_relative_import_resolves_with_the_extension_added() {
        let idx = index(&["src/a.ts", "src/b.ts"]);
        assert_eq!(resolve("src/a.ts", "./b", &idx), Ok(FileId(1)));
    }

    #[test]
    fn a_relative_import_walks_up_directories() {
        let idx = index(&["src/ui/a.ts", "src/domain/b.ts"]);
        assert_eq!(resolve("src/ui/a.ts", "../domain/b", &idx), Ok(FileId(1)));
    }

    #[test]
    fn a_directory_import_resolves_to_its_index_file() {
        let idx = index(&["src/a.ts", "src/thing/index.ts"]);
        assert_eq!(resolve("src/a.ts", "./thing", &idx), Ok(FileId(1)));
    }

    #[test]
    fn ts_wins_over_js_when_both_exist() {
        let idx = index(&["src/a.ts", "src/b.js", "src/b.ts"]);
        assert_eq!(resolve("src/a.ts", "./b", &idx), Ok(FileId(2)));
    }

    #[test]
    fn an_explicit_extension_is_honoured() {
        let idx = index(&["src/a.ts", "src/b.ts"]);
        assert_eq!(resolve("src/a.ts", "./b.ts", &idx), Ok(FileId(1)));
    }

    #[test]
    fn a_missing_relative_file_reports_no_such_file_not_external() {
        let idx = index(&["src/a.ts"]);
        assert_eq!(
            resolve("src/a.ts", "./gone", &idx),
            Err(UnresolvedReason::NoSuchFile("./gone".into()))
        );
    }

    #[test]
    fn candidate_paths_expands_aliases_the_same_way_resolution_does() {
        let project = JsProject {
            aliases: vec![PathAlias {
                prefix: "@/".into(),
                suffix: String::new(),
                has_wildcard: true,
                targets: vec!["src/*".into()],
                source: "tsconfig.json".into(),
                pattern: "@/*".into(),
            }],
            ..Default::default()
        };
        assert_eq!(
            candidate_paths("src/lib/ipc.ts", "@/bindings/Thing", &project),
            ["src/bindings/Thing"]
        );
        assert_eq!(
            candidate_paths("src/lib/ipc.ts", "./local", &JsProject::default()),
            ["src/lib/local"]
        );
    }

    #[test]
    fn a_stylesheet_import_is_an_asset_not_a_broken_import() {
        let idx = index(&["src/main.tsx"]);
        assert_eq!(
            resolve("src/main.tsx", "./index.css", &idx),
            Err(UnresolvedReason::Asset("css".into()))
        );
    }

    #[test]
    fn asset_detection_covers_the_usual_bundler_imports() {
        for (spec, ext) in [
            ("./logo.svg", "svg"),
            ("./data.json", "json"),
            ("~/styles/app.scss", "scss"),
            ("./shader.glsl", "glsl"),
            ("./font.woff2", "woff2"),
        ] {
            let idx = index(&["src/a.ts"]);
            assert_eq!(
                resolve("src/a.ts", spec, &idx),
                Err(UnresolvedReason::Asset(ext.into())),
                "{spec} should be an asset"
            );
        }
    }

    #[test]
    fn a_bundler_query_suffix_does_not_hide_the_asset_extension() {
        let idx = index(&["src/a.ts"]);
        assert_eq!(
            resolve("src/a.ts", "./worker.js?worker", &idx),
            Err(UnresolvedReason::NoSuchFile("./worker.js?worker".into())),
            "js is source, not an asset, even with a suffix"
        );
        assert_eq!(
            resolve("src/a.ts", "./icon.svg?raw", &idx),
            Err(UnresolvedReason::Asset("svg".into()))
        );
    }

    #[test]
    fn a_real_source_file_is_never_mistaken_for_an_asset() {
        let idx = index(&["src/a.ts", "src/b.ts"]);
        assert_eq!(resolve("src/a.ts", "./b", &idx), Ok(FileId(1)));
    }

    #[test]
    fn a_bare_specifier_is_external_with_the_package_name_extracted() {
        let idx = index(&["src/a.ts"]);
        assert_eq!(
            resolve("src/a.ts", "react", &idx),
            Err(UnresolvedReason::External("react".into()))
        );
        assert_eq!(
            resolve("src/a.ts", "lodash/debounce", &idx),
            Err(UnresolvedReason::External("lodash".into()))
        );
    }

    #[test]
    fn a_scoped_package_keeps_both_of_its_segments() {
        let idx = index(&["src/a.ts"]);
        assert_eq!(
            resolve("src/a.ts", "@tanstack/react-virtual", &idx),
            Err(UnresolvedReason::External("@tanstack/react-virtual".into()))
        );
        assert_eq!(
            resolve("src/a.ts", "@scope/pkg/deep/path", &idx),
            Err(UnresolvedReason::External("@scope/pkg".into()))
        );
    }

    #[test]
    fn a_paths_alias_resolves_through_its_wildcard() {
        let idx = index(&["src/a.ts", "src/lib/util.ts"]);
        let project = JsProject {
            aliases: vec![PathAlias {
                prefix: "@/".into(),
                suffix: String::new(),
                has_wildcard: true,
                targets: vec!["src/*".into()],
                source: "tsconfig.json".into(),
                pattern: String::new(),
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("src/a.ts", "@/lib/util", &idx, &project),
            Ok(FileId(1))
        );
    }

    #[test]
    fn an_alias_that_matches_but_points_nowhere_is_reported_as_such() {
        let idx = index(&["src/a.ts"]);
        let project = JsProject {
            aliases: vec![PathAlias {
                prefix: "@/".into(),
                suffix: String::new(),
                has_wildcard: true,
                targets: vec!["src/*".into()],
                source: "tsconfig.json".into(),
                pattern: String::new(),
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("src/a.ts", "@/missing", &idx, &project),
            Err(UnresolvedReason::UnmatchedAlias("@/missing".into()))
        );
    }

    #[test]
    fn a_non_wildcard_alias_matches_exactly() {
        let idx = index(&["src/a.ts", "src/config.ts"]);
        let project = JsProject {
            aliases: vec![PathAlias {
                prefix: "~config".into(),
                suffix: String::new(),
                has_wildcard: false,
                targets: vec!["src/config".into()],
                source: "tsconfig.json".into(),
                pattern: String::new(),
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("src/a.ts", "~config", &idx, &project),
            Ok(FileId(1))
        );
    }

    #[test]
    fn base_url_makes_a_bare_path_importable() {
        let idx = index(&["src/a.ts", "src/lib/x.ts"]);
        let project = JsProject {
            base_urls: vec!["src".into()],
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("src/a.ts", "lib/x", &idx, &project),
            Ok(FileId(1))
        );
    }

    #[test]
    fn a_workspace_package_resolves_across_the_monorepo() {
        let idx = index(&[
            "apps/web/a.ts",
            "packages/ui/index.ts",
            "packages/ui/button.ts",
        ]);
        let mut workspace_packages = HashMap::new();
        workspace_packages.insert("@app/ui".to_string(), "packages/ui".to_string());
        let project = JsProject {
            workspace_packages,
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("apps/web/a.ts", "@app/ui", &idx, &project),
            Ok(FileId(1))
        );
        assert_eq!(
            resolve_specifier("apps/web/a.ts", "@app/ui/button", &idx, &project),
            Ok(FileId(2))
        );
    }

    #[test]
    fn a_longer_alias_prefix_wins_over_a_shorter_one() {
        let idx = index(&["src/a.ts", "src/ui/x.ts", "other/x.ts"]);
        let mut project = JsProject {
            aliases: vec![
                PathAlias {
                    prefix: "@app/".into(),
                    suffix: String::new(),
                    has_wildcard: true,
                    targets: vec!["other/*".into()],
                    source: "tsconfig.json".into(),
                    pattern: String::new(),
                },
                PathAlias {
                    prefix: "@app/ui/".into(),
                    suffix: String::new(),
                    has_wildcard: true,
                    targets: vec!["src/ui/*".into()],
                    source: "tsconfig.json".into(),
                    pattern: String::new(),
                },
            ],
            ..Default::default()
        };
        project
            .aliases
            .sort_by_key(|a| std::cmp::Reverse(a.prefix.len()));
        assert_eq!(
            resolve_specifier("src/a.ts", "@app/ui/x", &idx, &project),
            Ok(FileId(1))
        );
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_do_not_break_tsconfig_reading() {
        let raw = r#"{
  // the compiler options
  "compilerOptions": {
    "baseUrl": ".", /* inline */
    "paths": {
      "@/*": ["src/*"],
    }
  }
}"#;
        let src = jsonc::strip_comments(raw);
        assert_eq!(
            jsonc::string_field(&src, "baseUrl", 0).as_deref(),
            Some(".")
        );
        let mut aliases = Vec::new();
        collect_aliases(&src, "", "tsconfig.json", &mut aliases);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].prefix, "@/");
        assert_eq!(aliases[0].targets, ["src/*"]);
    }

    #[test]
    fn a_url_inside_a_string_is_not_mistaken_for_a_comment() {
        let src = jsonc::strip_comments(r#"{"homepage": "https://example.com/x"}"#);
        assert!(src.contains("https://example.com/x"), "got: {src}");
    }

    #[test]
    fn multiple_alias_targets_are_tried_in_order() {
        let idx = index(&["src/a.ts", "fallback/x.ts"]);
        let project = JsProject {
            aliases: vec![PathAlias {
                prefix: "@/".into(),
                suffix: String::new(),
                has_wildcard: true,
                targets: vec!["primary/*".into(), "fallback/*".into()],
                source: "tsconfig.json".into(),
                pattern: String::new(),
            }],
            ..Default::default()
        };
        assert_eq!(
            resolve_specifier("src/a.ts", "@/x", &idx, &project),
            Ok(FileId(1))
        );
    }
}

pub fn audit_aliases(
    root: &Path,
    project: &JsProject,
    index: &FileIndex,
    specifiers: &[String],
) -> Vec<AliasAudit> {
    project
        .aliases
        .iter()
        .map(|alias| {
            let matches_scan = alias.targets.iter().any(|target| {
                let stem = target.trim_end_matches('*').trim_end_matches('/');
                index.contains(stem)
                    || !try_candidate_cased(stem, index).is_none()
                    || index_has_prefix(index, stem)
            });
            let on_disk = alias.targets.iter().any(|target| {
                let stem = target.trim_end_matches('*').trim_end_matches('/');
                if stem.is_empty() {
                    return true;
                }
                let base = crate::paths::rel_to_path(root, stem);
                base.exists()
                    || EXTENSIONS
                        .iter()
                        .any(|ext| base.with_extension(ext.trim_start_matches('.')).exists())
            });
            let used_by = specifiers
                .iter()
                .filter(|spec| matches_alias(alias, spec))
                .count() as u32;

            AliasAudit {
                pattern: alias.pattern.clone(),
                targets: alias.targets.clone(),
                source: alias.source.clone(),
                used_by,
                resolves: matches_scan || on_disk,
            }
        })
        .filter(|a| !a.resolves || a.used_by == 0)
        .collect()
}

fn index_has_prefix(index: &FileIndex, stem: &str) -> bool {
    if stem.is_empty() {
        return true;
    }
    let prefix = format!("{stem}/");
    (0..index.len()).any(|i| {
        index
            .path(FileId(i as u32))
            .is_some_and(|p| p.starts_with(&prefix))
    })
}

fn matches_alias(alias: &PathAlias, specifier: &str) -> bool {
    if alias.has_wildcard {
        specifier.len() >= alias.prefix.len() + alias.suffix.len()
            && specifier.starts_with(&alias.prefix)
            && specifier.ends_with(&alias.suffix)
    } else {
        specifier == alias.prefix
    }
}

#[cfg(test)]
mod alias_audit_tests {
    use super::*;
    use crate::model::Language;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = crate::paths::rel_to_path(dir, rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn audit(dir: &Path, files: &[&str], specifiers: &[&str]) -> Vec<AliasAudit> {
        let paths: Vec<String> = files.iter().map(|f| (*f).to_owned()).collect();
        let languages = vec![Language::TypeScript; paths.len()];
        let index = FileIndex::new(paths.clone(), languages);
        let mut all = paths.clone();
        all.push("tsconfig.json".to_owned());
        let project = build_project(dir, &index, &all);
        let specs: Vec<String> = specifiers.iter().map(|s| (*s).to_owned()).collect();
        audit_aliases(dir, &project, &index, &specs)
    }

    #[test]
    fn an_alias_pointing_at_nothing_is_reported_as_broken() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@gone/*":["src/gone/*"]}}}"#,
        );
        write(dir.path(), "src/real.ts", "export const a = 1;");

        let found = audit(dir.path(), &["src/real.ts"], &["@gone/thing"]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].pattern, "@gone/*");
        assert!(!found[0].resolves);
        assert_eq!(found[0].used_by, 1);
        assert_eq!(found[0].source, "tsconfig.json");
    }

    #[test]
    fn an_alias_whose_target_is_in_the_scan_is_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@app/*":["src/*"]}}}"#,
        );
        write(dir.path(), "src/a.ts", "export const a = 1;");
        assert!(audit(dir.path(), &["src/a.ts"], &["@app/a"]).is_empty());
    }

    #[test]
    fn an_alias_pointing_at_a_gitignored_directory_is_not_called_broken() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@gen/*":["generated/*"]}}}"#,
        );
        write(dir.path(), "src/a.ts", "export const a = 1;");

        write(dir.path(), "generated/api.ts", "export const g = 1;");

        let found = audit(dir.path(), &["src/a.ts"], &["@gen/api"]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_alias_nothing_imports_is_reported_as_unused_but_resolving() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@app/*":["src/*"]}}}"#,
        );
        write(dir.path(), "src/a.ts", "export const a = 1;");

        let found = audit(dir.path(), &["src/a.ts"], &["./relative"]);
        assert_eq!(found.len(), 1);
        assert!(found[0].resolves, "it works, it is just unused");
        assert_eq!(found[0].used_by, 0);
    }

    #[test]
    fn usage_is_counted_through_the_wildcard_not_by_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@x/*":["nowhere/*"]}}}"#,
        );
        let found = audit(dir.path(), &[], &["@x/one", "@x/two", "other"]);
        assert_eq!(found[0].used_by, 2);
    }

    #[test]
    fn a_repository_with_no_tsconfig_audits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/a.ts", "export const a = 1;");
        assert!(audit(dir.path(), &["src/a.ts"], &[]).is_empty());
    }
}
