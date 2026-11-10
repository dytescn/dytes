//! `dytes init` 各分支测试。

mod common;

use common::{assert_code, stderr, stdout, TestEnv};
use serde_json::json;

#[test]
fn init_app_creates_manifest() {
    let env = TestEnv::new();
    let out = env.run(&[
        "init", "--type", "app", "--name", "demo", "--version", "1.2.3",
    ]);
    assert_code(&out, 0);
    assert!(stdout(&out).contains("已生成"));

    let manifest = env.read_json("dytes.json");
    assert_eq!(manifest["name"], "demo");
    assert_eq!(manifest["type"], "app");
    assert_eq!(manifest["version"], "1.2.3");
    assert!(manifest["uuid"]
        .as_str()
        .is_some_and(|uuid| !uuid.is_empty()));
    assert!(manifest["exports"]
        .get("./target/release/demo.exe")
        .is_some());
}

#[test]
fn init_sys_uses_platform_exports() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "sys", "--name", "info_lib"]);
    assert_code(&out, 0);

    let manifest = env.read_json("dytes.json");
    assert_eq!(
        manifest["platform-exports"]["windows"]["./target/release/info_lib.dll"],
        "libary/info_lib.dll"
    );
    assert!(manifest.get("exports").is_none());
}

#[test]
fn init_web_has_init_scripts() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "web", "--name", "setting"]);
    assert_code(&out, 0);

    let manifest = env.read_json("dytes.json");
    assert_eq!(manifest["init"], json!(["./setup.json"]));
    assert_eq!(manifest["exports"]["./src/setup.json"], "setup.json");
}

#[test]
fn init_design_plugins_has_metadata() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "design_plugins", "--name", "cdr_check_txt"]);
    assert_code(&out, 0);

    let manifest = env.read_json("dytes.json");
    assert_eq!(manifest["plugins_type"], "plugins");
    assert_eq!(manifest["targets"], json!(["common"]));
    assert_eq!(manifest["use_ai"], false);
    assert_eq!(manifest["mcp"], "mcp.json");
}

#[test]
fn init_no_exports_creates_manifest_only() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "app", "--name", "demo", "--no-exports"]);
    assert_code(&out, 0);

    let manifest = env.read_json("dytes.json");
    assert!(manifest.get("exports").is_none());
    assert!(manifest.get("platform-exports").is_none());
}

#[test]
fn init_existing_manifest_requires_force() {
    let env = TestEnv::new();
    assert_code(&env.run(&["init", "--type", "app", "--name", "demo"]), 0);

    let out = env.run(&["init", "--type", "app", "--name", "demo"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("已存在"));

    let out = env.run(&["init", "--type", "app", "--name", "demo", "--force"]);
    assert_code(&out, 0);
}

#[test]
fn init_unknown_type_is_argument_error() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "unknown", "--name", "demo"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("未知包类型"));
}

#[test]
fn init_json_output_is_machine_readable() {
    let env = TestEnv::new();
    let out = env.run(&[
        "init", "--type", "sys", "--name", "info_lib", "--json",
    ]);
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["type"], "sys");
    assert_eq!(value["name"], "info_lib");
    assert_eq!(value["version"], "0.1.0");
    assert!(value["uuid"].as_str().is_some_and(|uuid| !uuid.is_empty()));
    assert!(value["path"].as_str().is_some());
}

#[test]
fn init_name_defaults_to_directory_name() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "app", "--dir", "pkgdir"]);
    assert_code(&out, 0);

    let manifest = env.read_json("pkgdir/dytes.json");
    assert_eq!(manifest["name"], "pkgdir");
}

#[test]
fn init_creates_missing_directory() {
    let env = TestEnv::new();
    let out = env.run(&["init", "--type", "model", "--name", "qwen", "--dir", "nested/dir"]);
    assert_code(&out, 0);
    assert!(env.root().join("nested/dir/dytes.json").is_file());
}
