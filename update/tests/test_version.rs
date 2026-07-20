use update::utils::version::{check_version, compare_versions};

#[test]
fn test_version_check() {
    assert!(check_version("1.0.0", "1.0.1"));
    assert!(!check_version("1.0.1", "1.0.0"));
    assert!(!check_version("1.0.0", "1.0.0"));
    assert!(check_version("0.9.0", "1.0.0"));
}

#[test]
fn test_version_check_with_prerelease() {
    assert!(check_version("1.0.0-alpha", "1.0.0"));
    assert!(!check_version("1.0.0", "1.0.0-alpha"));
    assert!(check_version("1.0.0-alpha", "1.0.0-beta"));
}

#[test]
fn test_compare_versions() {
    use std::cmp::Ordering;
    
    assert_eq!(compare_versions("1.0.0", "1.0.0"), Some(Ordering::Equal));
    assert_eq!(compare_versions("1.0.0", "1.0.1"), Some(Ordering::Less));
    assert_eq!(compare_versions("1.0.1", "1.0.0"), Some(Ordering::Greater));
    assert_eq!(compare_versions("invalid", "1.0.0"), None);
    assert_eq!(compare_versions("1.0.0", "invalid"), None);
}