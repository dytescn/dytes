use semver::Version;

pub fn check_version(cur_version: &str, remote_version: &str) -> bool {
    let cur = Version::parse(cur_version.trim_start_matches('v'));
    let remote = Version::parse(remote_version.trim_start_matches('v'));

    match (cur, remote) {
        (Ok(cur), Ok(remote)) => cur < remote, 
        _ => cur_version != remote_version,
    }
}
