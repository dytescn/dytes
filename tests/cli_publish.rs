//! `dytes publish` 的离线分支（`--dry-run` / 参数校验）测试。

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

#[test]
fn dry_run_reports_upload_plan() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["publish", "--dry-run", "--json"]);
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["manifest_only"], false);
    assert_eq!(value["type"], "app");
    assert_eq!(value["name"], "demo");
    assert_eq!(value["index"], "app/packages.json");
    assert_eq!(value["files"][0]["remote"], "app/demo/0.1.0/demo.exe");
    assert_eq!(value["files"][0]["exists"], true);
}

#[test]
fn dry_run_fails_when_export_is_missing() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["publish", "--dry-run"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("导出文件缺失"));
}

#[test]
fn dry_run_supports_manifest_only_packages() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({"name": "requirement", "version": "0.1.0", "type": "model"}),
    );

    let out = env.run(&["publish", "--dry-run", "--manifest-only", "--json"]);
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["manifest_only"], true);
    assert_eq!(value["files"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["index"], "models/packages.json");
}

#[test]
fn dry_run_rejects_path_traversal_in_export_name() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": {"./bin/demo.exe": "../evil.exe"}
        }),
    );
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["publish", "--dry-run"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("不支持的导出路径"));
}

#[test]
fn dry_run_uses_platform_exports() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "info_lib",
            "version": "0.1.0",
            "type": "sys",
            "platform-exports": {
                "windows": {"./target/release/info_lib.dll": "libary/info_lib.dll"}
            }
        }),
    );
    env.write("target/release/info_lib.dll", "dll");

    let out = env.run(&["publish", "--dry-run", "--json"]);
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["index"], "system/packages.json");
    let remote = value["files"][0]["remote"].as_str().unwrap();
    assert!(remote.starts_with("system/info_lib/0.1.0/windows/"), "{remote}");
}

#[test]
fn publish_missing_manifest_is_config_error() {
    let env = TestEnv::new();
    let out = env.run(&["publish", "--dry-run"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("dytes.json"));
}
