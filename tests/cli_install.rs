//! `dytes install` 的参数与离线校验分支测试（不触发网络安装）。

mod common;

use common::{assert_code, stderr, TestEnv};
use serde_json::json;

fn app_manifest() -> serde_json::Value {
    json!({
        "name": "demo",
        "version": "0.1.0",
        "type": "app",
        "exports": {"./bin/demo.exe": "demo.exe"}
    })
}

#[test]
fn install_requires_package_type() {
    let env = TestEnv::new();
    let out = env.run(&["install"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("请指定包类型"));
}

#[test]
fn install_app_points_to_update_command() {
    let env = TestEnv::new();
    let out = env.run(&["install", "app", "piksel"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("dytes update"));
}

#[test]
fn install_unknown_type_is_argument_error() {
    let env = TestEnv::new();
    let out = env.run(&["install", "nope", "pkg"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("暂不支持类型"));
}

#[test]
fn install_plugin_requires_name() {
    let env = TestEnv::new();
    let out = env.run(&["install", "plugin"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("缺少包名"));
}

#[test]
fn install_sys_requires_manifest() {
    let env = TestEnv::new();
    let out = env.run(&["install", "sys", "info_lib", "--offline"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("dytes.json"));
}

#[test]
fn install_sys_offline_requires_explicit_version() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["install", "sys", "info_lib", "--offline"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("离线安装需要显式版本"));
}

#[cfg(not(feature = "gui"))]
#[test]
fn install_gui_reports_missing_feature() {
    let env = TestEnv::new();
    let out = env.run(&["install", "--gui"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("未启用图形界面"));
}
