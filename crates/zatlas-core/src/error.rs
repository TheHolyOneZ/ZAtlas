use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("walking {root}: {source}")]
    Walk {
        root: PathBuf,
        #[source]
        source: ignore::Error,
    },

    #[error("bad glob pattern {pattern:?}: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    #[error("{path}: {source}")]
    Config {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cache: {0}")]
    Cache(String),

    #[error("cache schema version {found} is newer than this build understands ({expected}); delete .zatlas/ to re-scan")]
    CacheFromTheFuture { found: u32, expected: u32 },

    #[error("git: {0}")]
    Git(String),

    #[error("no git repository at {0} — history analysis is unavailable, structural analysis still works")]
    NotAGitRepo(PathBuf),

    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn other(message: impl std::fmt::Display) -> Self {
        Self::Other(message.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
