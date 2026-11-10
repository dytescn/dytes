//! `dytes check` 各分支测试。

mod common;

use common::{assert_code, stderr, stdout, TestEnv};
use serde_json::json;

fn app_manifest() -> serde_json::Value {
    json!({
        "name": "demo",
        "version": "0.1.0",
        "type": "app",
        "exports": {
            "./bin/demo.exe": "demo.exe"
        }
    })
}

fn sys_manifest() -> serde_json::Value {
    json!({
        "name": "info_lib",
        "version": "0.1.0",
        "type": "sys",
        "platform-exports": {
            "windows": {
                "./target/release/info_lib.dll": "libary/info_lib.dll"
            }
        }
    })
}

#[test]
fn check_passes_when_exports_exist() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["check"]);
    assert_code(&out, 0);
}

#[test]
fn check_fails_when_export_is_missing() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["check"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("清单校验失败"));
}

#[test]
fn check_rejects_type_mismatch() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["check", "--type", "sys"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("与 --type sys 不一致"));
}

#[test]
fn check_requires_exports_without_manifest_only() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({"name": "demo", "version": "0.1.0", "type": "app"}),
    );

    assert_code(&env.run(&["check"]), 3);
    assert_code(&env.run(&["check", "--manifest-only"]), 0);
}

#[test]
fn check_json_output_reports_failures() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["check", "--json"]);
    assert_code(&out, 3);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["passed"], false);
    assert!(value["checks"].as_array().is_some_and(|items| !items.is_empty()));
}

#[test]
fn check_uses_platform_exports_and_platform_flag() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &sys_manifest());
    env.write("target/release/info_lib.dll", "dll");

    assert_code(&env.run(&["check"]), 0);

    let out = env.run(&["check", "--platform", "linux"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("platform-exports"));
}

#[test]
fn check_missing_manifest_is_config_error() {
    let env = TestEnv::new();
    let out = env.run(&["check"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("dytes.json"));
}
