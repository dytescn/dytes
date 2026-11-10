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
