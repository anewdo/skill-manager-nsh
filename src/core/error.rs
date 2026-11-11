use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    UnsafePath,
    Database,
    Io,
    NotFound,
    Conflict,
    Network,
    Git,
    InvalidSkill,
    AmbiguousSkill,
    LocalModified,
    Cancelled,
    Unsupported,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::UnsafePath => "UNSAFE_PATH",
            Self::Database => "DATABASE",
            Self::Io => "IO",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::Network => "NETWORK",
            Self::Git => "GIT",
            Self::InvalidSkill => "INVALID_SKILL",
            Self::AmbiguousSkill => "AMBIGUOUS_SKILL",
            Self::LocalModified => "LOCAL_MODIFIED",
            Self::Cancelled => "CANCELLED",
            Self::Unsupported => "UNSUPPORTED",
            Self::Internal => "INTERNAL",
        }
    }
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            details: None,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn unsafe_path(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsafePath, message)
    }

    pub fn database(error: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Database, error.to_string())
    }

    pub fn internal(error: impl fmt::Display) -> Self {
        Self::new(ErrorCode::Internal, error.to_string())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, error.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::database(error)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(ErrorCode::Internal, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_structured_error_without_frontend_string_parsing() {
        let error = AppError::new(ErrorCode::Conflict, "already installed")
            .retryable(false)
            .with_details(serde_json::json!({ "skillId": "abc" }));
        let value = serde_json::to_value(error).unwrap();

        assert_eq!(value["code"], "CONFLICT");
        assert_eq!(value["message"], "already installed");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["details"]["skillId"], "abc");
    }
}
