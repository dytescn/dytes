//! CLI 集成测试的共享环境与断言工具。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 隔离的测试环境：`.dytes/run-<pid>/<test-name>/`，独立配置文件与固定平台。
pub struct TestEnv {
    dir: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let test_name = std::thread::current()
            .name()
            .unwrap_or("test")
            .replace("::", "-");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".dytes")
            .join(format!("run-{}", std::process::id()))
            .join(format!(
                "{test_name}-{}",
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&dir).expect("创建 .dytes 测试目录失败");
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        &self.dir
    }

    pub fn config_path(&self) -> PathBuf {
        self.root().join("config.toml")
    }

    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("创建测试目录失败");
        }
        std::fs::write(&path, content).expect("写入测试文件失败");
        path
    }

    pub fn write_json(&self, relative: &str, value: &serde_json::Value) -> PathBuf {
        self.write(relative, &serde_json::to_string_pretty(value).unwrap())
    }

    pub fn read_json(&self, relative: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(self.root().join(relative)).expect("读取测试文件失败");
        serde_json::from_str(&text).expect("解析测试 JSON 失败")
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dytes"));
        command.current_dir(self.root());
        command.env("DYTES_CONFIG", self.config_path());
        command.env("DYTES_PLATFORM", "windows");
        for key in [
            "DYTES_S3_ENDPOINT",
            "DYTES_S3_BUCKET",
            "DYTES_S3_REGION",
            "DYTES_S3_ACCESS_KEY",
            "DYTES_S3_SECRET_KEY",
            "PIKSEL_PLATFORM",
        ] {
            command.env_remove(key);
        }
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command()
            .args(args)
            .output()
            .expect("启动 dytes 失败")
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n")
}

pub fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

/// 断言退出码，并在失败时打印完整输出。
pub fn assert_code(output: &Output, expected: i32) {
    assert_eq!(
        code(output),
        expected,
        "退出码不符\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}
