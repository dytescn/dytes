//! `dytes config` 各分支测试（使用 DYTES_CONFIG 隔离配置文件）。

mod common;

use common::{assert_code, stdout, TestEnv};
use std::path::PathBuf;

#[test]
fn config_path_uses_environment_override() {
    let env = TestEnv::new();
    let out = env.run(&["config", "path", "--json"]);
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let path = PathBuf::from(value["path"].as_str().unwrap());
    assert_eq!(path, env.config_path());
}

#[test]
fn set_get_unset_roundtrip() {
    let env = TestEnv::new();
    assert_code(&env.run(&["config", "set", "s3.bucket", "my-bucket"]), 0);

    let out = env.run(&["config", "get", "s3.bucket"]);
    assert_code(&out, 0);
    assert_eq!(stdout(&out).trim(), "my-bucket");

    assert_code(&env.run(&["config", "unset", "s3.bucket"]), 0);
    let out = env.run(&["config", "get", "s3.bucket"]);
    assert_code(&out, 0);
    // 未配置时回退到内置默认值。
    assert_eq!(stdout(&out).trim(), "packages");
}

#[test]
fn sensitive_values_are_masked_by_default() {
    let env = TestEnv::new();
    assert_code(&env.run(&["config", "set", "s3.access_key", "AK-123"]), 0);
    assert_code(&env.run(&["config", "set", "s3.secret_key", "SK-456"]), 0);

    let out = env.run(&["config", "get", "s3.access_key"]);
    assert_code(&out, 0);
    assert_eq!(stdout(&out).trim(), "****");

    let out = env.run(&["config", "get", "s3.access_key", "--reveal"]);
    assert_code(&out, 0);
    assert_eq!(stdout(&out).trim(), "AK-123");

    let out = env.run(&["config", "list"]);
    assert_code(&out, 0);
    assert!(stdout(&out).contains("s3.secret_key = ****"));
    assert!(!stdout(&out).contains("SK-456"));
}

#[test]
fn config_json_outputs_are_machine_readable() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "s3.bucket", "my-bucket", "--json"]);
    assert_code(&out, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["saved"], true);

    let out = env.run(&["config", "get", "s3.bucket", "--json"]);
    assert_code(&out, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["key"], "s3.bucket");
    assert_eq!(value["value"], "my-bucket");

    let out = env.run(&["config", "list", "--json"]);
    assert_code(&out, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["s3"]["bucket"], "my-bucket");
    assert_eq!(value["s3"]["access_key"], "(未设置)");
    assert_eq!(value["platform"], "windows");
}

#[test]
fn platform_environment_overrides_config() {
    let env = TestEnv::new();
    let out = env
        .command()
        .env("DYTES_PLATFORM", "linux")
        .args(["config", "list", "--json"])
        .output()
        .unwrap();
    assert_code(&out, 0);

    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["platform"], "linux");
}

#[test]
fn unknown_config_keys_are_argument_errors() {
    let env = TestEnv::new();
    assert_code(&env.run(&["config", "get", "unknown", "x"]), 2);
    assert_code(&env.run(&["config", "set", "unknown", "x"]), 2);
    assert_code(&env.run(&["config", "unset", "unknown"]), 2);
}

#[test]
fn set_requires_a_value_for_non_sensitive_keys() {
    let env = TestEnv::new();
    let out = env.run(&["config", "set", "s3.bucket"]);
    assert_code(&out, 2);
}
