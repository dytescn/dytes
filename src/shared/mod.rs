//! dytes 共享核心：包清单、锁文件、平台、版本、凭证与模板。

pub mod config;
pub mod credentials;
pub mod error;
pub mod lock;
pub mod package;
pub mod platform;
pub mod templates;
pub mod version;

pub use error::{DytesError, Result};
