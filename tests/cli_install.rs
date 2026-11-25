//! `dytes install` 的参数与离线校验分支测试（不触发网络安装）。

mod common;

use common::{assert_code, stderr, stdout, TestEnv};
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
fn install_default_targets_core_piksel() {
    let env = TestEnv::new();

    // 不带类型时按 core piksel 处理：空目录先初始化清单，再进入依赖安装流程。
    let out = env.run(&["install", "--offline"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("离线安装需要显式版本"));
    assert!(stdout(&out).contains("已初始化应用清单"));

    let manifest = env.read_json("dytes.json");
    assert_eq!(manifest["type"], "app");
}

#[test]
fn install_app_rejects_existing_manifest() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["install", "app", "demo"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("已存在"));
    assert!(stderr(&out).contains("dytes update"));
}

#[test]
fn install_app_requires_name() {
    let env = TestEnv::new();

    let out = env.run(&["install", "app"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("缺少包名"));
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
fn install_sys_initializes_manifest_when_missing() {
    let env = TestEnv::new();

    let out = env.run(&["install", "sys", "info_lib", "--offline"]);
    // 清单缺失时先初始化 type=app 的 dytes.json，再继续原来的安装流程；
    // 离线且未指定版本，因此停在参数校验。
    assert_code(&out, 2);
    assert!(stderr(&out).contains("离线安装需要显式版本"));
    assert!(stdout(&out).contains("已初始化应用清单"));

    let manifest = env.read_json("dytes.json");
    assert_eq!(manifest["type"], "app");
    assert!(manifest["uuid"]
        .as_str()
        .is_some_and(|uuid| !uuid.is_empty()));
    let dir_name = env
        .root()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(manifest["name"].as_str(), Some(dir_name.as_str()));
}

#[test]
fn install_sys_offline_requires_explicit_version() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["install", "sys", "info_lib", "--offline"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("离线安装需要显式版本"));
}

#[test]
fn install_sys_relative_dir_resolves_manifest() {
    let env = TestEnv::new();
    env.write_json("app/dytes.json", &app_manifest());

    let out = env.run(&["install", "sys", "info_lib", "--dir", "app", "--offline"]);
    // 修复前会二次拼接成 app/app/dytes.json，返回 3。
    assert_code(&out, 2);
    assert!(stderr(&out).contains("离线安装需要显式版本"));
}

#[test]
fn install_sys_uses_app_dir_manifest() {
    let env = TestEnv::new();
    env.write_json("app/dytes.json", &app_manifest());

    // 未传 --dir 时，清单与应用根都应以 DYTES_APP_DIR 为准。
    let out = env
        .command()
        .env("DYTES_APP_DIR", env.root().join("app"))
        .args(["install", "sys", "info_lib", "--offline"])
        .output()
        .expect("启动 dytes 失败");
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
