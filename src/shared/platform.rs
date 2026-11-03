use std::collections::HashMap;

use crate::shared::config::DytesConfig;

/// 平台名归一化：把常见别名统一为 `windows` / `linux` / `macos`。
pub fn normalize_platform(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "win" | "win32" | "win64" | "windows" => "windows".to_string(),
        "mac" | "macos" | "darwin" | "osx" => "macos".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
}

/// 当前平台：windows / linux / macos。
pub fn current_platform() -> String {
    for key in ["DYTES_PLATFORM", "PIKSEL_PLATFORM"] {
        if let Ok(value) = std::env::var(key) {
            let value = normalize_platform(&value);
            if !value.is_empty() {
                return value;
            }
        }
    }
    normalize_platform(std::env::consts::OS)
}

/// 选择导出表；有 platform-exports 时必须命中当前平台。
pub fn select_exports<'a>(
    config: &'a DytesConfig,
    platform: &'a str,
) -> Option<(&'a HashMap<String, String>, Option<&'a str>)> {
    if config.platform_exports.is_empty() {
        Some((&config.exports, None))
    } else {
        let platform = normalize_platform(platform);
        config
            .platform_exports
            .get_key_value(&platform)
            .map(|(key, exports)| (exports, Some(key.as_str())))
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

    #[test]
    fn normalizes_platform_aliases() {
        assert_eq!(normalize_platform("Win32"), "windows");
        assert_eq!(normalize_platform("darwin"), "macos");
        assert_eq!(normalize_platform(" osx "), "macos");
        assert_eq!(normalize_platform("linux"), "linux");
        assert_eq!(normalize_platform("freebsd"), "freebsd");
    }
}
