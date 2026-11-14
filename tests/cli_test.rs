//! `dytes test` 本地自检与安装演练测试。

mod common;

use common::{assert_code, stderr, stdout, TestEnv};
use serde_json::json;

fn app_manifest() -> serde_json::Value {
    json!({
        "name": "demo",
        "uuid": "11111111-1111-1111-1111-111111111111",
        "version": "0.1.0",
        "type": "app",
        "exports": {
            "./bin/demo.exe": "demo.exe"
        }
    })
}

#[test]
fn test_rehearses_exports_and_writes_lock() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test", "--work-dir", "rehearsal", "--keep"]);
    assert_code(&out, 0);
    assert!(stdout(&out).contains("导出文件可落盘: demo.exe"));
    assert!(stdout(&out).contains("锁文件可序列化"));

    assert_eq!(
        std::fs::read_to_string(env.root().join(".dytes/rehearsal/demo.exe")).unwrap(),
        "binary"
    );
    let lock = env.read_json(".dytes/rehearsal/dytes.lock");
    assert_eq!(lock["package"][0]["name"], "demo");
    assert_eq!(lock["package"][0]["type"], "app");
    assert_eq!(lock["package"][0]["loader"][0][1], "demo.exe");
    assert!(env.root().join(".dytes/rehearsal/dytes.json").is_file());
}

#[test]
fn test_cleans_rehearsal_by_default() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 0);
    assert!(env.root().join(".dytes").is_dir());
    assert!(!env.root().join(".dytes/rehearsal").exists());
}

#[test]
fn test_fails_when_export_is_missing() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("包自检失败"));
}

#[test]
fn test_rejects_unsafe_export_destination() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": { "./bin/demo.exe": "../evil.exe" }
        }),
    );
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("路径不安全"));
    assert!(!env.root().join("evil.exe").exists());
}

#[test]
fn test_rejects_missing_init_script() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo_web",
            "version": "0.1.0",
            "type": "web",
            "exports": { "./dist/index.html": "index.html" },
            "init": ["./setup.json"]
        }),
    );
    env.write("dist/index.html", "<html></html>");

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("init 脚本"));
    assert!(stdout(&out).contains("源文件缺失"));
}

#[test]
fn test_manifest_only_passes_without_exports() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({"name": "training", "version": "0.1.0", "type": "model"}),
    );

    let out = env.run(&[
        "test",
        "--manifest-only",
        "--work-dir",
        "rehearsal",
        "--keep",
    ]);
    assert_code(&out, 0);
    assert!(env.root().join(".dytes/rehearsal/dytes.lock").is_file());
}

#[test]
fn test_json_output_reports_files_and_checks() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test", "--json", "--work-dir", "rehearsal", "--keep"]);
    assert_code(&out, 0);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(value["passed"], true);
    assert_eq!(value["name"], "demo");
    assert_eq!(value["kept"], true);
    assert_eq!(value["gitignore"], "created");
    assert!(value["dytes_dir"]
        .as_str()
        .is_some_and(|path| path.ends_with(".dytes")));
    assert!(value["env_dir"]
        .as_str()
        .is_some_and(|path| path.replace('\\', "/").ends_with(".dytes/env")));
    assert!(value["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty()));
    assert_eq!(value["checks"][0]["label"], "dytes.json 可解析");
}

#[test]
fn test_platform_exports_must_match() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "info_lib",
            "version": "0.1.0",
            "type": "sys",
            "platform-exports": {
                "windows": { "./target/release/info_lib.dll": "info_lib.dll" }
            }
        }),
    );
    env.write("target/release/info_lib.dll", "dll");

    let out = env.run(&["test", "--platform", "linux"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("platform-exports"));
}

#[test]
fn test_sys_layout_does_not_double_libary_prefix() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "info_lib",
            "version": "0.1.0",
            "type": "sys",
            "platform-exports": {
                "windows": { "./target/release/info_lib.dll": "libary/info_lib.dll" }
            }
        }),
    );
    env.write("target/release/info_lib.dll", "dll");

    let out = env.run(&["test", "--work-dir", "rehearsal", "--keep"]);
    assert_code(&out, 0);
    assert!(env
        .root()
        .join(".dytes/rehearsal/libary/info_lib.dll")
        .is_file());
    assert!(!env.root().join(".dytes/rehearsal/libary/libary").exists());
    assert!(env
        .root()
        .join(".dytes/rehearsal/libary/info_lib/dytes.json")
        .is_file());
}

#[test]
fn test_sys_plain_filename_lands_in_libary() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "info_lib",
            "version": "0.1.0",
            "type": "sys",
            "exports": { "./target/release/info_lib.dll": "info_lib.dll" }
        }),
    );
    env.write("target/release/info_lib.dll", "dll");

    let out = env.run(&["test", "--work-dir", "rehearsal", "--keep"]);
    assert_code(&out, 0);
    assert!(env
        .root()
        .join(".dytes/rehearsal/libary/info_lib.dll")
        .is_file());
}

#[test]
fn test_rejects_unsafe_source_path() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": { "../outside.exe": "outside.exe" }
        }),
    );
    std::fs::write(env.root().join("../outside.exe"), "outside").unwrap();

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("源路径不安全"));
    assert!(!env.root().join(".dytes/rehearsal/outside.exe").exists());
}

#[test]
fn test_rejects_unsafe_package_name() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "../evil",
            "version": "0.1.0",
            "type": "web",
            "exports": { "./index.html": "index.html" }
        }),
    );
    env.write("index.html", "<html></html>");

    let out = env.run(&["test", "--work-dir", "rehearsal"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("包名"));
    assert!(!env.root().join(".dytes/rehearsal").exists());
}

#[test]
fn test_creates_dytes_dir_and_gitignore() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test"]);
    assert_code(&out, 0);
    assert!(env.root().join(".dytes").is_dir());
    assert!(env.root().join(".dytes/env").is_dir());

    let gitignore = std::fs::read_to_string(env.root().join(".gitignore")).unwrap();
    assert_eq!(gitignore.matches(".dytes/").count(), 1);
}

#[test]
fn test_gitignore_is_appended_once() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");
    env.write(
        ".gitignore",
        "target
",
    );

    assert_code(&env.run(&["test"]), 0);
    assert_code(&env.run(&["test"]), 0);

    let gitignore = std::fs::read_to_string(env.root().join(".gitignore")).unwrap();
    assert_eq!(gitignore.matches(".dytes/").count(), 1);
    assert!(gitignore.starts_with(
        "target
"
    ));
}

#[test]
fn test_dependency_is_copied_into_dytes_env() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": { "./bin/demo.exe": "demo.exe" },
            "test-dependency": {
                "sys": { "info_lib": "0.1.0" },
                "web": { "setting": "0.2.0" }
            }
        }),
    );
    env.write("bin/demo.exe", "binary");
    env.write("libary/info_lib.dll", "dll");
    env.write("libary/info_lib/dytes.json", "{\"name\":\"info_lib\"}");
    env.write("web/setting/index.html", "<html></html>");
    env.write_json(
        "dytes.lock",
        &json!({
            "package": [{
                "name": "info_lib",
                "version": "0.1.0",
                "type": "sys",
                "loader": [["src", "libary/info_lib.dll"]]
            }]
        }),
    );

    let out = env.run(&["test", "--work-dir", "rehearsal", "--keep"]);
    assert_code(&out, 0);
    assert!(env.root().join(".dytes/env/libary/info_lib.dll").is_file());
    assert!(env
        .root()
        .join(".dytes/env/libary/info_lib/dytes.json")
        .is_file());
    assert!(env
        .root()
        .join(".dytes/env/web/setting/index.html")
        .is_file());

    // 原有安装目录不被修改。
    assert!(env.root().join("libary/info_lib.dll").is_file());
}

#[test]
fn test_missing_test_dependency_fails() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": { "./bin/demo.exe": "demo.exe" },
            "test-dependency": { "sys": { "missing_lib": "0.1.0" } }
        }),
    );
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test"]);
    assert_code(&out, 3);
    assert!(stdout(&out).contains("未找到已安装的依赖"));
}

#[test]
fn test_invalid_test_dependency_type_is_rejected() {
    let env = TestEnv::new();
    env.write_json(
        "dytes.json",
        &json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app",
            "exports": { "./bin/demo.exe": "demo.exe" },
            "test-dependency": { "unknown": { "x": "1.0.0" } }
        }),
    );
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test"]);
    assert_code(&out, 3);
    assert!(stderr(&out).contains("test-dependency"));
}

#[test]
fn test_work_dir_must_stay_inside_dytes() {
    let env = TestEnv::new();
    env.write_json("dytes.json", &app_manifest());
    env.write("bin/demo.exe", "binary");

    let out = env.run(&["test", "--work-dir", "../outside"]);
    assert_code(&out, 2);
    assert!(stderr(&out).contains("--work-dir"));
    assert!(!env.root().join("outside").exists());
}
