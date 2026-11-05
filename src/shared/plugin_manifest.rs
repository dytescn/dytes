//! 设计插件包（`type=design_plugins`）的发布前清单校验。
//!
//! 规则与文档仓库的 `sysinfo/design_plugins/dytes-manifest.md` §3 对齐；
//! `dytes check` 与 `dytes publish` 共用本模块，避免发布前只靠人工核对。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::shared::error::{DytesError, Result};

/// `plugins_type` 允许值。
pub const KNOWN_PLUGINS_TYPES: [&str; 4] = [
    "chat_think",
    "chat_reply",
    "plugins",
    "plugins_coreldraw",
];

/// `targets` 允许值。
pub const KNOWN_TARGETS: [&str; 5] =
    ["common", "coreldraw", "photoshop", "autocad", "vectorcraft"];

const MAX_NAME: usize = 120;
const MAX_DESCRIPTION: usize = 500;
const MAX_CONTENT: usize = 8000;

/// 一条校验结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

/// 一次清单校验的全部结果。
#[derive(Debug, Default)]
pub struct CheckReport {
    pub items: Vec<CheckItem>,
}

impl CheckReport {
    pub fn pass(&mut self, label: impl Into<String>, detail: impl Into<String>) {
        self.record(label, true, detail);
    }

    pub fn fail(&mut self, label: impl Into<String>, detail: impl Into<String>) {
        self.record(label, false, detail);
    }

    pub fn record(&mut self, label: impl Into<String>, ok: bool, detail: impl Into<String>) {
        self.items.push(CheckItem {
            label: label.into(),
            ok,
            detail: detail.into(),
        });
    }

    pub fn extend(&mut self, other: CheckReport) {
        self.items.extend(other.items);
    }

    pub fn failures(&self) -> impl Iterator<Item = &CheckItem> {
        self.items.iter().filter(|item| !item.ok)
    }

    pub fn failure_count(&self) -> usize {
        self.failures().count()
    }

    pub fn passed(&self) -> bool {
        self.failure_count() == 0
    }

    /// 按 `PASS / FAIL` 逐项打印，并输出汇总。
    pub fn print(&self) {
        for item in &self.items {
            let status = if item.ok { "PASS" } else { "FAIL" };
            if item.detail.is_empty() {
                println!("{status} {}", item.label);
            } else {
                println!("{status} {}  {}", item.label, item.detail);
            }
        }
        println!();
        match self.failure_count() {
            0 => println!("OK: 包结构检查通过"),
            count => println!("FAILED: {count} 项"),
        }
    }
}

/// 校验设计插件清单。
///
/// `dir` 为包根目录，`manifest` 为已解析的 `dytes.json`，
/// `platform` 为当前平台（用于选择 `platform-exports`）。
pub fn check_design_plugin(dir: &Path, manifest: &Value, platform: &str) -> CheckReport {
    let mut report = CheckReport::default();

    check_identity(&mut report, manifest);
    check_plugins_type(&mut report, manifest);
    check_targets(&mut report, manifest);
    let use_ai = check_use_ai(&mut report, manifest);
    let export_targets = check_exports(&mut report, dir, manifest, platform);
    check_mcp(&mut report, manifest, &export_targets);
    check_skills(&mut report, manifest, &export_targets, use_ai);

    report
}

/// 校验通过时返回 `Ok`，否则打印明细并返回配置错误。
pub fn ensure_design_plugin(dir: &Path, manifest: &Value, platform: &str) -> Result<()> {
    let report = check_design_plugin(dir, manifest, platform);
    report.print();
    if report.passed() {
        Ok(())
    } else {
        Err(DytesError::config(format!(
            "设计插件清单校验失败: {} 项",
            report.failure_count()
        )))
    }
}

fn check_identity(report: &mut CheckReport, manifest: &Value) {
    let name = manifest.get("name").and_then(Value::as_str).unwrap_or("");
    report.record("name 非空", !name.trim().is_empty(), name);

    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("");
    report.record("version 非空", !version.trim().is_empty(), version);
    report.record(
        "version 为合法 semver",
        semver::Version::parse(version).is_ok(),
        version,
    );

    let pkg_type = manifest.get("type").and_then(Value::as_str).unwrap_or("");
    report.record(
        "type == design_plugins",
        pkg_type == "design_plugins",
        pkg_type,
    );
}

fn check_plugins_type(report: &mut CheckReport, manifest: &Value) {
    let value = manifest.get("plugins_type").and_then(Value::as_str);
    let detail = value.unwrap_or("<缺失>").to_string();
    let ok = value.is_some_and(|value| KNOWN_PLUGINS_TYPES.contains(&value));
    report.record("plugins_type 取值合法", ok, detail);
}

fn check_targets(report: &mut CheckReport, manifest: &Value) {
    let Some(targets) = manifest.get("targets") else {
        report.fail("targets 为非空字符串数组", "<缺失>");
        return;
    };
    let detail = targets.to_string();
    let Some(list) = targets.as_array() else {
        report.fail("targets 为非空字符串数组", detail);
        return;
    };
    let strings: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
    let shape_ok = !list.is_empty()
        && strings.len() == list.len()
        && strings.iter().all(|item| !item.trim().is_empty());
    report.record("targets 为非空字符串数组", shape_ok, detail.clone());
    if !shape_ok {
        return;
    }

    report.record(
        "targets 均为小写",
        strings.iter().all(|item| *item == item.to_lowercase()),
        detail.clone(),
    );

    let normalized: Vec<String> = strings.iter().map(|item| item.trim().to_lowercase()).collect();
    let unique: HashSet<&String> = normalized.iter().collect();
    report.record("targets 无重复", unique.len() == normalized.len(), detail.clone());

    let unknown: Vec<&str> = normalized
        .iter()
        .map(String::as_str)
        .filter(|item| !KNOWN_TARGETS.contains(item))
        .collect();
    report.record(
        "targets 取值合法",
        unknown.is_empty(),
        if unknown.is_empty() {
            normalized.join(", ")
        } else {
            format!("未知: {}", unknown.join(", "))
        },
    );

    let common_mixed = normalized.iter().any(|item| item == "common") && normalized.len() > 1;
    report.record("common 只能单独出现", !common_mixed, detail.clone());
}

fn check_use_ai(report: &mut CheckReport, manifest: &Value) -> Option<bool> {
    let value = manifest.get("use_ai");
    let parsed = value.and_then(Value::as_bool);
    report.record(
        "use_ai 为布尔值",
        parsed.is_some(),
        value
            .map(ToString::to_string)
            .unwrap_or_else(|| "<缺失>".to_string()),
    );
    parsed
}

/// 选择导出表并校验每个导出文件；返回“远程文件名 -> 本地路径”。
fn check_exports(
    report: &mut CheckReport,
    dir: &Path,
    manifest: &Value,
    platform: &str,
) -> HashMap<String, PathBuf> {
    let platform_exports = manifest
        .get("platform-exports")
        .and_then(Value::as_object)
        .filter(|map| !map.is_empty());

    let exports = match platform_exports {
        Some(map) => match map.get(platform).and_then(Value::as_object) {
            Some(exports) => Some(exports),
            None => {
                report.fail("当前平台导出", format!("platform-exports 缺少 {platform}"));
                return HashMap::new();
            }
        },
        None => manifest.get("exports").and_then(Value::as_object),
    };

    let Some(exports) = exports else {
        report.fail("exports 非空", "<缺失>");
        return HashMap::new();
    };
    report.record(
        "exports 非空",
        !exports.is_empty(),
        format!("{} 个", exports.len()),
    );

    let mut targets = HashMap::new();
    for (local, remote) in exports {
        let Some(remote) = remote.as_str() else {
            report.fail(format!("exports 文件名合法: {local}"), "映射目标不是字符串");
            continue;
        };
        if remote.contains('/') || remote.contains('\\') {
            report.fail(
                format!("exports 文件名合法: {remote}"),
                "只能是不带目录的文件名",
            );
            continue;
        }

        let path = dir.join(local.trim_start_matches("./"));
        report.record(
            format!("exports 文件存在: {local}"),
            path.is_file(),
            path.display().to_string(),
        );
        targets.insert(remote.trim_start_matches("./").to_string(), path);
    }
    targets
}

fn check_mcp(
    report: &mut CheckReport,
    manifest: &Value,
    export_targets: &HashMap<String, PathBuf>,
) {
    let Some(path) = export_field(report, "mcp", "mcp.json", manifest, export_targets) else {
        return;
    };
    let Some(value) = read_json(report, "mcp.json", &path) else {
        return;
    };

    let tools = value.get("tools");
    let tools_ok = tools
        .and_then(Value::as_array)
        .is_some_and(|list| !list.is_empty());
    report.record(
        "mcp tools 为非空数组",
        tools_ok,
        tools
            .map(ToString::to_string)
            .unwrap_or_else(|| "<缺失>".to_string()),
    );

    let Some(list) = tools.and_then(Value::as_array) else {
        return;
    };
    for tool in list {
        let Some(tool) = tool.as_str() else {
            report.fail("mcp 工具文件名合法", tool.to_string());
            continue;
        };
        if tool.contains('/') || tool.contains('\\') {
            report.fail(
                format!("mcp 文件名合法: {tool}"),
                "只能是不带目录的文件名",
            );
            continue;
        }
        let exported = export_targets.get(tool).is_some_and(|path| path.is_file());
        report.record(
            format!("mcp 工具已导出: {tool}"),
            exported,
            export_targets
                .get(tool)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<未导出>".to_string()),
        );
    }
}

fn check_skills(
    report: &mut CheckReport,
    manifest: &Value,
    export_targets: &HashMap<String, PathBuf>,
    use_ai: Option<bool>,
) {
    match use_ai {
        Some(true) => {
            let Some(path) = export_field(report, "skills", "skills.json", manifest, export_targets)
            else {
                return;
            };
            let Some(value) = read_json(report, "skills.json", &path) else {
                return;
            };
            let skills = value.get("skills");
            let skills_ok = skills
                .and_then(Value::as_array)
                .is_some_and(|list| !list.is_empty());
            report.record(
                "skills 为非空数组",
                skills_ok,
                skills
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "<缺失>".to_string()),
            );

            let Some(list) = skills.and_then(Value::as_array) else {
                return;
            };
            let mut seen = HashSet::new();
            for (index, skill) in list.iter().enumerate() {
                check_skill(report, index, skill, &mut seen);
            }
        }
        Some(false) => {
            let declared = manifest.get("skills").is_some();
            let exported = export_targets.contains_key("skills.json");
            report.record(
                "use_ai=false 时不得声明 skills",
                !declared,
                if declared { "已声明" } else { "" },
            );
            report.record(
                "use_ai=false 时不得导出 skills.json",
                !exported,
                if exported { "已导出" } else { "" },
            );
        }
        None => {}
    }
}

/// 校验 `mcp` / `skills` 字段、导出映射与本地文件，返回可读取的路径。
fn export_field(
    report: &mut CheckReport,
    field: &str,
    expected: &str,
    manifest: &Value,
    export_targets: &HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    let value = manifest.get(field).and_then(Value::as_str);
    report.record(
        format!("{field} == {expected}"),
        value == Some(expected),
        value.unwrap_or("<缺失>").to_string(),
    );

    let Some(path) = export_targets.get(expected) else {
        report.fail(
            format!("{expected} 已导出"),
            "<未在 exports 中映射>".to_string(),
        );
        return None;
    };
    report.pass(format!("{expected} 已导出"), "");
    report.record(
        format!("{expected} 存在"),
        path.is_file(),
        path.display().to_string(),
    );
    if path.is_file() {
        Some(path.clone())
    } else {
        None
    }
}

fn read_json(report: &mut CheckReport, label: &str, path: &Path) -> Option<Value> {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => {
                report.pass(format!("{label} 可解析"), "");
                Some(value)
            }
            Err(error) => {
                report.fail(format!("{label} 可解析"), error.to_string());
                None
            }
        },
        Err(error) => {
            report.fail(format!("{label} 可解析"), error.to_string());
            None
        }
    }
}

fn check_skill(
    report: &mut CheckReport,
    index: usize,
    skill: &Value,
    seen: &mut HashSet<String>,
) {
    let Some(skill) = skill.as_object() else {
        report.fail(format!("skills[{index}] 为对象"), "不是 JSON 对象");
        return;
    };

    let id = skill.get("id").and_then(Value::as_str).unwrap_or("");
    let id_ok = !id.trim().is_empty();
    report.record(format!("skills[{index}].id 非空"), id_ok, id);
    if id_ok {
        report.record(
            format!("skills[{index}].id 唯一"),
            seen.insert(id.to_string()),
            id,
        );
    }

    let name = skill.get("name").and_then(Value::as_str);
    report.record(
        format!("skills[{index}].name 合法"),
        name.is_some_and(|value| {
            let len = value.chars().count();
            len > 0 && len <= MAX_NAME
        }),
        name.map(|value| value.chars().count().to_string())
            .unwrap_or_else(|| "<缺失>".to_string()),
    );

    let description = skill.get("description").and_then(Value::as_str);
    report.record(
        format!("skills[{index}].description 合法"),
        description.is_some_and(|value| value.chars().count() <= MAX_DESCRIPTION),
        description
            .map(|value| value.chars().count().to_string())
            .unwrap_or_else(|| "<缺失>".to_string()),
    );

    let content = skill.get("content").and_then(Value::as_str);
    report.record(
        format!("skills[{index}].content 合法"),
        content.is_some_and(|value| {
            let len = value.chars().count();
            len > 0 && len <= MAX_CONTENT
        }),
        content
            .map(|value| value.chars().count().to_string())
            .unwrap_or_else(|| "<缺失>".to_string()),
    );

    let category = skill.get("category").and_then(Value::as_str).unwrap_or("");
    report.record(
        format!("skills[{index}].category 非空"),
        !category.trim().is_empty(),
        category,
    );

    let params_schema = skill.get("params_schema");
    report.record(
        format!("skills[{index}].params_schema 为对象"),
        params_schema.is_none_or(Value::is_object),
        params_schema
            .map(ToString::to_string)
            .unwrap_or_else(|| "<缺失>".to_string()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write(path: impl AsRef<Path>, content: &str) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn manifest() -> Value {
        json!({
            "name": "cdr_check_txt",
            "version": "0.2.0",
            "type": "design_plugins",
            "plugins_type": "plugins_coreldraw",
            "targets": ["coreldraw"],
            "use_ai": true,
            "mcp": "mcp.json",
            "skills": "skills.json",
            "exports": {
                "./target/release/cdr_check_txt.dll": "cdr_check_txt.dll",
                "./mcp/mcp.json": "mcp.json",
                "./mcp/text_list.json": "text_list.json",
                "./skills.json": "skills.json"
            }
        })
    }

    fn prepare(dir: &Path, manifest: &Value) {
        write(dir.join("target/release/cdr_check_txt.dll"), "dll");
        write(dir.join("mcp/mcp.json"), r#"{"tools":["text_list.json"]}"#);
        write(dir.join("mcp/text_list.json"), "{}");
        write(
            dir.join("skills.json"),
            r#"{"skills":[{"id":"export","name":"导出","description":"","content":"内容","category":"design"}]}"#,
        );
        let _ = manifest;
    }

    #[test]
    fn accepts_valid_package() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &manifest());
        let report = check_design_plugin(dir.path(), &manifest(), "windows");
        assert!(report.passed(), "{:#?}", report.items);
    }

    #[test]
    fn rejects_missing_metadata() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &manifest());
        let report = check_design_plugin(
            dir.path(),
            &json!({"name": "x", "version": "0.1", "type": "design_plugins"}),
            "windows",
        );
        let labels: Vec<&str> = report.failures().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"plugins_type 取值合法"));
        assert!(labels.contains(&"targets 为非空字符串数组"));
        assert!(labels.contains(&"use_ai 为布尔值"));
        assert!(labels.contains(&"mcp == mcp.json"));
    }

    #[test]
    fn rejects_common_mixed_with_other_targets() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &manifest());
        let mut value = manifest();
        value["targets"] = json!(["common", "coreldraw"]);
        let report = check_design_plugin(dir.path(), &value, "windows");
        let labels: Vec<&str> = report.failures().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"common 只能单独出现"));
    }

    #[test]
    fn rejects_use_ai_false_with_skills() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &manifest());
        let mut value = manifest();
        value["use_ai"] = json!(false);
        let report = check_design_plugin(dir.path(), &value, "windows");
        let labels: Vec<&str> = report.failures().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"use_ai=false 时不得声明 skills"));
        assert!(labels.contains(&"use_ai=false 时不得导出 skills.json"));
    }

    #[test]
    fn rejects_missing_mcp_tool_export() {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &manifest());
        write(dir.path().join("mcp/mcp.json"), r#"{"tools":["missing.json"]}"#);
        let report = check_design_plugin(dir.path(), &manifest(), "windows");
        let labels: Vec<&str> = report.failures().map(|item| item.label.as_str()).collect();
        assert!(labels.iter().any(|label| label.contains("mcp 工具已导出")));
    }
}
