use std::collections::HashMap;

use crate::utils::dytes::DytesConfig;

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

/// 当前平台发布产物的可执行后缀。
pub fn executable_suffix(platform: &str) -> Option<&'static str> {
    match normalize_platform(platform).as_str() {
        "windows" => Some(".exe"),
        "linux" => Some(".bin"),
        "macos" => Some(".app"),
        _ => None,
    }
}

/// 判断文件名是否像平台可执行产物（无后缀，或 `.exe` / `.bin` / `.app`）。
fn looks_like_executable(filename: &str) -> bool {
    match std::path::Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        None => true,
        Some(ext) => matches!(ext.as_str(), "exe" | "bin" | "app"),
    }
}

/// 校验平台专用导出文件的后缀。
///
/// 只关注可执行产物：若导出里存在无后缀 / `.exe` / `.bin` / `.app` 的文件，
/// 则至少要求一个文件以当前平台后缀结尾。`.html`、`.dll`、`.so`、`.json`
/// 等资源文件不参与校验；纯资源包直接通过。
pub fn validate_platform_exports(
    exports: &HashMap<String, String>,
    platform: &str,
) -> std::result::Result<(), String> {
    let Some(suffix) = executable_suffix(platform) else {
        return Ok(());
    };
    let binaries: Vec<&str> = exports
        .values()
        .map(String::as_str)
        .filter(|name| looks_like_executable(name))
        .collect();
    if binaries.is_empty() {
        return Ok(());
    }
    if binaries
        .iter()
        .any(|name| name.to_ascii_lowercase().ends_with(suffix))
    {
        return Ok(());
    }
    Err(format!(
        "导出文件没有 {} 产物（当前平台 {}）；实际导出: {}。\
         请检查 dytes.json 的 platform-exports 是否按平台写上了正确后缀。",
        suffix,
        normalize_platform(platform),
        binaries.join(", ")
    ))
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

    fn exports(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(source, dest)| (source.to_string(), dest.to_string()))
            .collect()
    }

    #[test]
    fn validates_platform_executable_suffix() {
        assert!(validate_platform_exports(
            &exports(&[("./target/release/dytes", "dytes.bin")]),
            "linux"
        )
        .is_ok());
        assert!(validate_platform_exports(
            &exports(&[("./target/release/dytes", "dytes.exe")]),
            "windows"
        )
        .is_ok());
        // 资源文件不参与校验
        assert!(validate_platform_exports(
            &exports(&[("./dist/index.html", "index.html")]),
            "linux"
        )
        .is_ok());
    }

    #[test]
    fn rejects_missing_platform_suffix() {
        let result = validate_platform_exports(
            &exports(&[("./target/release/dytes", "dytes")]),
            "linux",
        );
        assert!(result.unwrap_err().contains(".bin"));

        let result = validate_platform_exports(
            &exports(&[("./target/release/piksel.exe", "piksel.exe")]),
            "linux",
        );
        assert!(result.unwrap_err().contains(".bin"));
    }
}
