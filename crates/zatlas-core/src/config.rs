use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct ZatlasConfig {
    pub include: Vec<String>,

    pub exclude: Vec<String>,

    pub entrypoints: Vec<String>,
    #[serde(rename = "layers")]
    pub layers: Vec<Layer>,

    pub rules: Vec<String>,

    pub lsp: crate::lsp::LspSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct Layer {
    pub name: String,
    pub paths: Vec<String>,

    #[serde(default)]
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LayerRule {
    pub from: String,
    pub to: String,
}

impl ZatlasConfig {
    pub const FILE_NAME: &'static str = "zatlas.toml";

    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(Self::FILE_NAME);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::from_toml(&text).map_err(|source| CoreError::Config { path, source }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(CoreError::io(&path, source)),
        }
    }

    pub fn from_toml(text: &str) -> std::result::Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn all_rule_strings(&self) -> Vec<String> {
        self.rules
            .iter()
            .chain(self.layers.iter().flat_map(|l| l.rules.iter()))
            .cloned()
            .collect()
    }

    pub fn layer_rules(&self) -> Vec<LayerRule> {
        let declared: Vec<&str> = self.layers.iter().map(|l| l.name.as_str()).collect();
        self.all_rule_strings()
            .iter()
            .filter_map(|raw| {
                let (from, to) = raw.split_once("->")?;
                let from = from.trim();
                let to = to.trim();
                if from.is_empty() || to.is_empty() {
                    return None;
                }
                if !declared.contains(&from) || !declared.contains(&to) {
                    return None;
                }
                Some(LayerRule {
                    from: from.to_owned(),
                    to: to.to_owned(),
                })
            })
            .collect()
    }

    pub fn invalid_rules(&self) -> Vec<String> {
        let valid: Vec<String> = self
            .layer_rules()
            .iter()
            .map(|r| format!("{} -> {}", r.from, r.to))
            .collect();
        self.all_rule_strings()
            .into_iter()
            .filter(|raw| {
                let normalised = raw
                    .split_once("->")
                    .map(|(f, t)| format!("{} -> {}", f.trim(), t.trim()))
                    .unwrap_or_else(|| raw.clone());
                !valid.contains(&normalised)
            })
            .collect()
    }
}

fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_array(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|v| toml_string(v)).collect();
    format!("[{}]", items.join(", "))
}

impl ZatlasConfig {
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# Written by ZAtlas. Safe to edit by hand.\n");
        out.push_str("# https://github.com/TheHolyOneZ/ZAtlas\n\n");

        if !self.include.is_empty() {
            out.push_str("# Globs limiting the scan. Empty means everything the walk finds.\n");
            out.push_str(&format!("include = {}\n", toml_array(&self.include)));
        }
        if !self.exclude.is_empty() {
            out.push_str("# Removed from the scan, applied after `include`.\n");
            out.push_str(&format!("exclude = {}\n", toml_array(&self.exclude)));
        }
        if !self.entrypoints.is_empty() {
            out.push_str(
                "# Files treated as roots. Without these, every `main` looks like an orphan.\n",
            );
            out.push_str(&format!(
                "entrypoints = {}\n",
                toml_array(&self.entrypoints)
            ));
        }
        if !self.include.is_empty() || !self.exclude.is_empty() || !self.entrypoints.is_empty() {
            out.push('\n');
        }

        let rules = self.all_rule_strings();
        if !rules.is_empty() {
            out.push_str("# Allowed dependency directions. Anything else between two declared\n");
            out.push_str("# layers is a violation.\n");
            out.push_str(&format!("rules = {}\n\n", toml_array(&rules)));
        }

        for layer in &self.layers {
            out.push_str("[[layers]]\n");
            out.push_str(&format!("name = {}\n", toml_string(&layer.name)));
            out.push_str(&format!("paths = {}\n\n", toml_array(&layer.paths)));
        }

        if self.lsp != crate::lsp::LspSettings::default() {
            out.push_str("[lsp]\n");
            out.push_str(&format!(
                "mode = {}\n",
                toml_string(match self.lsp.mode {
                    crate::lsp::LspMode::Off => "off",
                    crate::lsp::LspMode::Repair => "repair",
                    crate::lsp::LspMode::Verify => "verify",
                })
            ));
            if !self.lsp.servers.is_empty() {
                out.push_str(&format!("servers = {}\n", toml_array(&self.lsp.servers)));
            }
        }

        out
    }

    pub fn save(&self, root: &Path) -> Result<PathBuf> {
        let path = root.join(Self::FILE_NAME);
        std::fs::write(&path, self.to_toml()).map_err(|e| CoreError::io(&path, e))?;
        Ok(path)
    }
}

pub fn cache_dir(root: &Path) -> PathBuf {
    root.join(".zatlas")
}

pub fn cache_db_path(root: &Path) -> PathBuf {
    cache_dir(root).join("cache.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::{LspMode, LspSettings};

    const SAMPLE: &str = r#"
include = ["src/**", "crates/**"]
exclude = ["**/generated/**", "**/*.test.ts"]
entrypoints = ["src/main.tsx", "crates/api/src/main.rs"]

[[layers]]
name = "ui"
paths = ["src/ui/**"]

[[layers]]
name = "domain"
paths = ["src/domain/**"]

[[layers]]
name = "data"
paths = ["src/data/**"]

rules = ["ui -> domain", "domain -> data"]
"#;

    #[test]
    fn parses_the_shape_the_spec_documents() {
        let cfg = ZatlasConfig::from_toml(SAMPLE).expect("sample config should parse");
        assert_eq!(cfg.include, ["src/**", "crates/**"]);
        assert_eq!(cfg.exclude.len(), 2);
        assert_eq!(cfg.entrypoints.len(), 2);
        assert_eq!(cfg.layers.len(), 3);
        assert_eq!(cfg.layers[0].name, "ui");
        assert_eq!(cfg.layers[0].paths, ["src/ui/**"]);
    }

    #[test]
    fn an_empty_config_is_valid_because_most_repos_will_not_have_one() {
        let cfg = ZatlasConfig::from_toml("").expect("empty config should parse");
        assert_eq!(cfg, ZatlasConfig::default());
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ZatlasConfig::load(dir.path()).expect("missing config should be fine");
        assert_eq!(cfg, ZatlasConfig::default());
    }

    #[test]
    fn load_reads_a_real_file_from_the_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ZatlasConfig::FILE_NAME), SAMPLE).unwrap();
        let cfg = ZatlasConfig::load(dir.path()).unwrap();
        assert_eq!(cfg.layers.len(), 3);
    }

    #[test]
    fn a_malformed_config_reports_the_path_not_just_the_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ZatlasConfig::FILE_NAME), "include = [oops").unwrap();
        let err = ZatlasConfig::load(dir.path()).unwrap_err();
        assert!(
            err.to_string().contains("zatlas.toml"),
            "error should name the file, got: {err}"
        );
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_silently_ignored() {
        let err = ZatlasConfig::from_toml("includes = [\"src/**\"]").unwrap_err();
        assert!(err.to_string().contains("includes"), "got: {err}");
    }

    #[test]
    fn rules_parse_into_directed_pairs() {
        let cfg = ZatlasConfig::from_toml(SAMPLE).unwrap();
        let rules = cfg.layer_rules();
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0],
            LayerRule {
                from: "ui".into(),
                to: "domain".into()
            }
        );
        assert_eq!(
            rules[1],
            LayerRule {
                from: "domain".into(),
                to: "data".into()
            }
        );
    }

    #[test]
    fn whitespace_around_the_arrow_does_not_matter() {
        let cfg = ZatlasConfig::from_toml(
            "[[layers]]\nname=\"a\"\npaths=[]\n[[layers]]\nname=\"b\"\npaths=[]\nrules=[\"a->b\"]",
        )
        .unwrap();
        assert_eq!(
            cfg.layer_rules(),
            [LayerRule {
                from: "a".into(),
                to: "b".into()
            }]
        );
    }

    #[test]
    fn a_rule_naming_an_undeclared_layer_is_dropped_and_reported() {
        let cfg = ZatlasConfig::from_toml(
            "[[layers]]\nname=\"ui\"\npaths=[]\nrules=[\"ui -> domian\", \"ui -> ui\"]",
        )
        .unwrap();
        assert_eq!(
            cfg.layer_rules(),
            [LayerRule {
                from: "ui".into(),
                to: "ui".into()
            }]
        );
        assert_eq!(cfg.invalid_rules(), ["ui -> domian"]);
    }

    #[test]
    fn rules_are_accepted_at_the_document_root_too() {
        let cfg = ZatlasConfig::from_toml(
            "rules = [\"ui -> domain\"]\n[[layers]]\nname=\"ui\"\npaths=[]\n[[layers]]\nname=\"domain\"\npaths=[]",
        )
        .unwrap();
        assert_eq!(
            cfg.layer_rules(),
            [LayerRule {
                from: "ui".into(),
                to: "domain".into()
            }]
        );
    }

    #[test]
    fn rules_written_after_the_layers_blocks_still_apply() {
        let cfg = ZatlasConfig::from_toml(SAMPLE).unwrap();
        assert!(cfg.rules.is_empty(), "the root key really is empty here");
        assert_eq!(
            cfg.layers[2].rules.len(),
            2,
            "TOML bound them to the last layer"
        );
        assert_eq!(cfg.layer_rules().len(), 2, "and they still take effect");
    }

    #[test]
    fn rules_split_across_both_positions_are_merged() {
        let cfg = ZatlasConfig::from_toml(
            "rules = [\"ui -> domain\"]\n[[layers]]\nname=\"ui\"\npaths=[]\n[[layers]]\nname=\"domain\"\npaths=[]\nrules=[\"domain -> ui\"]",
        )
        .unwrap();
        assert_eq!(cfg.layer_rules().len(), 2);
    }

    #[test]
    fn a_rule_without_an_arrow_is_reported_as_invalid() {
        let cfg =
            ZatlasConfig::from_toml("[[layers]]\nname=\"ui\"\npaths=[]\nrules=[\"ui domain\"]")
                .unwrap();
        assert!(cfg.layer_rules().is_empty());
        assert_eq!(cfg.invalid_rules(), ["ui domain"]);
    }

    #[test]
    fn lsp_is_off_unless_the_repository_opts_in() {
        let cfg = ZatlasConfig::from_toml(SAMPLE).unwrap();
        assert_eq!(cfg.lsp.mode, LspMode::Off);
    }

    #[test]
    fn an_lsp_section_parses_the_way_it_reads() {
        let cfg = ZatlasConfig::from_toml(
            "[lsp]\nmode = \"repair\"\ntimeoutMs = 1500\nservers = [\"rust-analyzer\"]",
        )
        .unwrap();
        assert_eq!(cfg.lsp.mode, LspMode::Repair);
        assert_eq!(cfg.lsp.timeout_ms, 1500);
        assert_eq!(cfg.lsp.servers, ["rust-analyzer"]);

        assert_eq!(cfg.lsp.startup_ms, LspSettings::default().startup_ms);
    }

    #[test]
    fn the_cache_lives_inside_the_scanned_repository() {
        let root = Path::new("/tmp/repo");
        assert_eq!(cache_dir(root), Path::new("/tmp/repo/.zatlas"));
        assert_eq!(
            cache_db_path(root),
            Path::new("/tmp/repo/.zatlas/cache.sqlite")
        );
    }
    #[test]
    fn a_generated_config_parses_back_to_the_same_thing() {
        let original = ZatlasConfig::from_toml(SAMPLE).unwrap();
        let round = ZatlasConfig::from_toml(&original.to_toml()).unwrap();
        assert_eq!(round.layers.len(), original.layers.len());
        assert_eq!(round.include, original.include);
        assert_eq!(round.exclude, original.exclude);
        assert_eq!(round.entrypoints, original.entrypoints);
        assert_eq!(round.layer_rules(), original.layer_rules());
    }

    #[test]
    fn rules_are_written_before_the_layer_blocks() {
        let cfg = ZatlasConfig::from_toml(SAMPLE).unwrap();
        let text = cfg.to_toml();
        let rules_at = text.find("rules =").expect("rules are written");
        let layers_at = text.find("[[layers]]").expect("layers are written");
        assert!(rules_at < layers_at, "got:\n{text}");

        let back = ZatlasConfig::from_toml(&text).unwrap();
        assert_eq!(back.rules.len(), 2);
    }

    #[test]
    fn an_empty_config_writes_a_document_that_still_parses() {
        let text = ZatlasConfig::default().to_toml();
        assert_eq!(
            ZatlasConfig::from_toml(&text).unwrap(),
            ZatlasConfig::default()
        );
    }

    #[test]
    fn a_quote_in_a_glob_is_escaped_rather_than_breaking_the_file() {
        let cfg = ZatlasConfig {
            include: vec!["src/we\"ird/**".into()],
            ..Default::default()
        };
        let back = ZatlasConfig::from_toml(&cfg.to_toml()).unwrap();
        assert_eq!(back.include, ["src/we\"ird/**"]);
    }

    #[test]
    fn a_backslash_in_a_glob_survives_the_round_trip() {
        let cfg = ZatlasConfig {
            exclude: vec!["gen\\\\out/**".into()],
            ..Default::default()
        };
        let back = ZatlasConfig::from_toml(&cfg.to_toml()).unwrap();
        assert_eq!(back.exclude, ["gen\\\\out/**"]);
    }

    #[test]
    fn the_lsp_section_is_only_written_when_it_differs_from_the_default() {
        assert!(!ZatlasConfig::default().to_toml().contains("[lsp]"));
        let cfg = ZatlasConfig {
            lsp: crate::lsp::LspSettings {
                mode: LspMode::Repair,
                ..Default::default()
            },
            ..Default::default()
        };
        let text = cfg.to_toml();
        assert!(text.contains("[lsp]"));
        assert_eq!(
            ZatlasConfig::from_toml(&text).unwrap().lsp.mode,
            LspMode::Repair
        );
    }

    #[test]
    fn saving_writes_the_file_where_load_will_find_it() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ZatlasConfig::from_toml(SAMPLE).unwrap();
        let path = cfg.save(dir.path()).unwrap();
        assert!(path.ends_with(ZatlasConfig::FILE_NAME));
        assert_eq!(
            ZatlasConfig::load(dir.path()).unwrap().layers.len(),
            cfg.layers.len()
        );
    }
}
