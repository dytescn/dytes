//! `dytes update` 的离线 / 参数分支测试（空目录与缺清单场景，不触发网络）。

mod common;

use common::{assert_code, stderr, TestEnv};

#[test]
fn update_check_requires_manifest() {
    let env = TestEnv::new();
    let out = env.run(&["update", "--check"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("检查更新失败"));
}

#[test]
fn update_startup_requires_manifest() {
    let env = TestEnv::new();
    let out = env.run(&["update", "--startup"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("检查启动更新状态失败"));
}

#[test]
fn update_with_type_filter_requires_manifest() {
    let env = TestEnv::new();
    let out = env.run(&["update", "--type", "sys"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("更新失败"));
}

#[test]
fn update_offline_requires_manifest() {
    let env = TestEnv::new();
    let out = env.run(&["update", "--offline"]);
    assert_code(&out, 1);
    assert!(stderr(&out).contains("更新失败"));
}

#[test]
fn update_help_succeeds() {
    let env = TestEnv::new();
    let out = env.run(&["update", "--help"]);
    assert_code(&out, 0);
}
