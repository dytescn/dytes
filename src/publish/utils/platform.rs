use std::collections::HashMap;

use crate::publish::utils::dytes::DytesConfig;

/// 当前平台名：windows / linux / macos
/// 可用 PIKSEL_PLATFORM 环境变量覆盖，便于测试其它平台。
pub fn current_platform() -> String {
    if let Ok(value) = std::env::var("PIKSEL_PLATFORM") {
        let value = value.trim().to_lowercase();
        if !value.is_empty() {
            return value;
        }
    }
    std::env::consts::OS.to_string()
}

/// 选择当前平台的导出文件；没有平台专用配置时回退到通用 exports。
pub fn select_exports<'a>(
    config: &'a DytesConfig,
    platform: &'a str,
) -> Option<(&'a HashMap<String, String>, Option<&'a str>)> {
    if config.platform_exports.is_empty() {
        Some((&config.exports, None))
    } else {
        config
            .platform_exports
            .get(platform)
            .map(|exports| (exports, Some(platform)))
    }
}

/// 远程包路径：平台专用时带 /<platform>/，否则保持旧布局。
pub fn remote_base(prefix: &str, version: &str, platform: Option<&str>) -> String {
    match platform {
        Some(platform) => format!("{}/{}/{}/", prefix, version, platform),
        None => format!("{}/{}/", prefix, version),
    }
}
