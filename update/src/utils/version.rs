use semver::Version;

/// 检查是否需要更新
pub fn check_version(current: &str, latest: &str) -> bool {
    match (Version::parse(current), Version::parse(latest)) {
        (Ok(curr), Ok(latest)) => latest > curr,
        _ => false,
    }
}

/// 比较版本号，返回比较结果
pub fn compare_versions(v1: &str, v2: &str) -> Option<std::cmp::Ordering> {
    match (Version::parse(v1), Version::parse(v2)) {
        (Ok(ver1), Ok(ver2)) => Some(ver1.cmp(&ver2)),
        _ => None,
    }
}
