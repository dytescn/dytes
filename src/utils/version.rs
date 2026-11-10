use semver::Version;

pub fn parse(value: &str) -> Option<Version> {
    Version::parse(value).ok()
}

pub fn needs_update(current: &str, latest: &str) -> bool {
    match (parse(current), parse(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

pub fn compare(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(parse(left)?.cmp(&parse(right)?))
}

/// 检查 `latest` 是否比 `current` 新。任一版本无法解析时返回 `false`。
pub fn check_version(current: &str, latest: &str) -> bool {
    needs_update(current, latest)
}

/// 去掉 `^` / `~` 前缀并校验 semver；非法时返回 `None`。
pub fn clean_declared(value: &str) -> Option<String> {
    let cleaned = value.trim_start_matches('^').trim_start_matches('~');
    parse(cleaned).map(|_| cleaned.to_string())
}

/// 解析 `name` / `name@version`。
///
/// 返回 `(包名, 版本)`；包名为空时返回 `None`。
/// 同时提供 `--version` 时优先使用显式版本。
pub fn parse_name_version(
    raw: Option<&str>,
    explicit_version: Option<&str>,
) -> Option<(String, Option<String>)> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let (name, inline_version) = match raw.split_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (raw.to_string(), None),
    };

    Some((
        name,
        explicit_version.map(str::to_string).or(inline_version),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(needs_update("0.1.0", "0.2.0"));
        assert!(!needs_update("0.2.0", "0.2.0"));
        assert_eq!(compare("1.0.0", "1.0.0"), Some(std::cmp::Ordering::Equal));
        assert_eq!(compare("bad", "1.0.0"), None);
    }
}
