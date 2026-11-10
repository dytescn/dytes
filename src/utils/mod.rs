//! 跨模块共用工具：清单模型、平台、版本、校验、凭据、模板、配置与 S3 客户端。

pub mod checksum;
pub mod config;
pub mod credentials;
pub mod dytes;
pub mod desktop;
pub mod error;
pub mod ffi;
pub mod package;
pub mod platform;
pub mod plugin_manifest;
pub mod plugin_lib;
pub mod s3;
pub mod templates;
pub mod version;

#[cfg(feature = "gui")]
pub mod gui;
