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
