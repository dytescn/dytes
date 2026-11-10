//! `dytes self-update` 的参数分支测试（不触发下载）。

mod common;

use common::{assert_code, stderr, TestEnv};

#[test]
fn self_update_rejects_unknown_source() {
    let env = TestEnv::new();
    let out = env.run(&["self-update", "--source", "ftp", "--check"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("--source 只支持"));
}

#[test]
fn self_update_help_succeeds() {
    let env = TestEnv::new();
    let out = env.run(&["self-update", "--help"]);
    assert_code(&out, 0);
}
