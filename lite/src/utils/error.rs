use std::fmt;

/// 统一错误：退出码 + 可读信息。
#[derive(Debug, Clone)]
pub struct DytesError {
    pub code: i32,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, DytesError>;

impl DytesError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// 参数错误（退出码 2）。
    pub fn args(message: impl Into<String>) -> Self {
        Self::new(2, message)
    }

    /// 配置 / 清单错误（退出码 3）。
    pub fn config(message: impl Into<String>) -> Self {
        Self::new(3, message)
    }

    /// 一般失败（退出码 1）。
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(1, message)
    }
}

impl fmt::Display for DytesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DytesError {}

impl From<std::io::Error> for DytesError {
    fn from(error: std::io::Error) -> Self {
        Self::failed(error.to_string())
    }
}

impl From<serde_json::Error> for DytesError {
    fn from(error: serde_json::Error) -> Self {
        Self::config(format!("JSON 解析失败: {error}"))
    }
}

impl From<toml::de::Error> for DytesError {
    fn from(error: toml::de::Error) -> Self {
        Self::config(format!("TOML 解析失败: {error}"))
    }
}

impl From<toml::ser::Error> for DytesError {
    fn from(error: toml::ser::Error) -> Self {
        Self::config(format!("TOML 序列化失败: {error}"))
    }
}
