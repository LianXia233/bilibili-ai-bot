//! 统一错误类型。

use std::fmt;

#[derive(Debug, Clone)]
pub enum AppError {
    Config(String),
    Http(String),
    Api { code: i64, msg: String },
    Json(String),
    Io(String),
    Crypto(String),
    Llm(String),
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(s) => write!(f, "配置错误: {s}"),
            AppError::Http(s) => write!(f, "网络错误: {s}"),
            AppError::Api { code, msg } => write!(f, "API 错误(code={code}): {msg}"),
            AppError::Json(s) => write!(f, "JSON 错误: {s}"),
            AppError::Io(s) => write!(f, "IO 错误: {s}"),
            AppError::Crypto(s) => write!(f, "加密错误: {s}"),
            AppError::Llm(s) => write!(f, "模型调用失败: {s}"),
            AppError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Json(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}

impl From<base64::DecodeError> for AppError {
    fn from(e: base64::DecodeError) -> Self {
        AppError::Crypto(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

/// 把任意错误转成 AppError::Other。
#[allow(dead_code)]
pub fn other<E: std::fmt::Display>(e: E) -> AppError {
    AppError::Other(e.to_string())
}
