use serde::Serialize;
use ts_rs::TS;
use zatlas_core::CoreError;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub remedy: String,
    pub retryable: bool,
}

impl CommandError {
    pub fn new(
        code: impl Into<String>,
        message: impl std::fmt::Display,
        remedy: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
            remedy: remedy.into(),
            retryable,
        }
    }
}

impl From<CoreError> for CommandError {
    fn from(e: CoreError) -> Self {
        match &e {
            CoreError::NotAGitRepo(_) => CommandError::new(
                "not_a_git_repo",
                &e,
                "Structural analysis works without git. To get churn, ownership and \
                 co-change coupling, open a folder that is a git repository.",
                false,
            ),
            CoreError::CacheFromTheFuture { .. } => CommandError::new(
                "cache_from_the_future",
                &e,
                "Delete the .zatlas folder inside the repository and scan again.",
                false,
            ),
            CoreError::Io { .. } | CoreError::Walk { .. } => CommandError::new(
                "io",
                &e,
                "Check that the path still exists and that you have permission to read it.",
                true,
            ),
            CoreError::Glob { .. } | CoreError::Config { .. } => CommandError::new(
                "config",
                &e,
                "Fix the pattern in zatlas.toml and re-scan.",
                false,
            ),
            CoreError::Cache(_) => CommandError::new(
                "cache",
                &e,
                "Delete the .zatlas folder inside the repository and scan again.",
                true,
            ),
            CoreError::Git(_) => CommandError::new(
                "git",
                &e,
                "The repository history could not be read. Structural analysis is unaffected.",
                true,
            ),
            CoreError::Other(_) => CommandError::new("internal", &e, "Please report this.", false),
        }
    }
}

pub type Response<T> = std::result::Result<T, CommandError>;
