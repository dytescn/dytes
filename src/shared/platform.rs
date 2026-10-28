use std::collections::HashMap;

use crate::shared::config::DytesConfig;

/// 当前平台：windows / linux / macos。
pub fn current_platform() -> String {
    for key in ["DYTES_PLATFORM", "PIKSEL_PLATFORM"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim().to_lowercase();
            if !value.is_empty() {
                return value;
            }
        }
    }
    std::env::consts::OS.to_string()
}

/// 选择导出表；有 platform-exports 时必须命中当前平台。
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

pub fn remote_base(prefix: &str, version: &str, platform: Option<&str>) -> String {
    match platform {
        Some(platform) => format!("{prefix}/{version}/{platform}/"),
        None => format!("{prefix}/{version}/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_base_uses_platform() {
        assert_eq!(remote_base("system", "0.1.0", None), "system/0.1.0/");
        assert_eq!(
            remote_base("system", "0.1.0", Some("windows")),
            "system/0.1.0/windows/"
        );
    }
}
