//! S3 客户端与凭据读取：install / update / publish 共用。

use crate::utils::credentials::{effective_s3, ResolvedS3};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Config, Region};
use aws_sdk_s3::Client;

pub fn settings() -> ResolvedS3 {
    effective_s3()
}

pub fn bucket() -> String {
    settings().bucket
}

/// 未配置凭证时给用户的可操作提示。
const MISSING_CREDENTIALS: &str = "S3 凭证未配置：请执行 `dytes config set s3.access_key <AK>` \
     与 `dytes config set s3.secret_key <SK>`，或设置环境变量 \
     DYTES_S3_ACCESS_KEY / DYTES_S3_SECRET_KEY；本地构建要内置凭证，需在 \
     `cargo build` 时设置 UPYUN_API_KEY / UPYUN_API_SECRET";

fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).map_or(true, str::is_empty)
}

fn credentials_missing(settings: &ResolvedS3) -> bool {
    is_blank(&settings.access_key) || is_blank(&settings.secret_key)
}

/// 读取 / 下载 / 列举前检查凭证，避免把「没有凭证」报成含糊的 `service error`。
pub fn ensure_credentials() -> Result<(), String> {
    if credentials_missing(&settings()) {
        Err(MISSING_CREDENTIALS.to_string())
    } else {
        Ok(())
    }
}

/// 创建访问又拍云 S3 的客户端。
pub async fn client() -> Client {
    let credentials = Credentials::new(
        settings().access_key.unwrap_or_default(),
        settings().secret_key.unwrap_or_default(),
        None,
        None,
        "custom",
    );

    let shared = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(settings().endpoint)
        .region(Region::new("us-east-1"))
        .credentials_provider(credentials)
        .load()
        .await;

    let config = Config::from(&shared)
        .to_builder()
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(access_key: Option<&str>, secret_key: Option<&str>) -> ResolvedS3 {
        ResolvedS3 {
            endpoint: "https://example.com".to_string(),
            bucket: "bucket".to_string(),
            region: "us-east-1".to_string(),
            access_key: access_key.map(str::to_string),
            secret_key: secret_key.map(str::to_string),
        }
    }

    #[test]
    fn detects_missing_credentials() {
        assert!(!credentials_missing(&resolved(Some("AK"), Some("SK"))));
        assert!(credentials_missing(&resolved(None, Some("SK"))));
        assert!(credentials_missing(&resolved(Some("AK"), None)));
        assert!(credentials_missing(&resolved(Some(" "), Some("SK"))));
        assert!(credentials_missing(&resolved(Some("AK"), Some(""))));
    }
}
