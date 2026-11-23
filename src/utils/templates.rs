use crate::utils::dytes::PackageType;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn init_manifest(
    pkg_type: PackageType,
    name: &str,
    version: &str,
    include_exports: bool,
) -> Value {
    let base = json!({
        "name": name,
        "uuid": Uuid::new_v4().to_string(),
        "version": version,
        "description": format!("{name} package"),
        "author": "funxdata",
        "type": pkg_type.as_str(),
    });
    let mut manifest = base;
    let object = manifest.as_object_mut().expect("manifest object");
    if pkg_type == PackageType::DesignPlugins {
        // 设计插件的分类字段与是否产出二进制无关，始终写入。
        object.insert("plugins_type".to_string(), json!("plugins"));
        object.insert("targets".to_string(), json!(["common"]));
        object.insert("use_ai".to_string(), json!(false));
        object.insert("mcp".to_string(), json!("mcp.json"));
    }
    if !include_exports {
        return manifest;
    }
    // 只生成导出字段骨架，不猜测产物路径：不同项目（Rust / C++ / 模型 / Web）
    // 的产物位置和平台后缀差异很大，自动填写的路径往往不可用。
    // 保留字段本身，方便用户直接填写。
    match pkg_type {
        PackageType::Sys => {
            object.insert("platform-exports".to_string(), json!({}));
        }
        PackageType::Web => {
            object.insert("exports".to_string(), json!({}));
            object.insert("init".to_string(), json!(["./setup.json"]));
        }
        PackageType::App
        | PackageType::Core
        | PackageType::DesignPlugins
        | PackageType::DesignSoft
        | PackageType::Model => {
            object.insert("exports".to_string(), json!({}));
        }
    }
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::dytes::DytesConfig;

    #[test]
    fn all_templates_are_valid() {
        for pkg_type in [
            PackageType::App,
            PackageType::Core,
            PackageType::Sys,
            PackageType::Web,
            PackageType::DesignPlugins,
            PackageType::DesignSoft,
            PackageType::Model,
        ] {
            let value = init_manifest(pkg_type, "demo", "0.1.0", true);
            let config: DytesConfig = serde_json::from_value(value).unwrap();
            // `init` 只生成空导出骨架，校验时需允许“仅清单”包。
            config.validate_with_options(true).unwrap();
            assert_eq!(config.package_type().unwrap(), pkg_type);
            assert!(config.uuid.as_deref().is_some_and(|uuid| !uuid.is_empty()));
        }
    }

    #[test]
    fn templates_default_to_empty_exports() {
        for pkg_type in [
            PackageType::App,
            PackageType::Core,
            PackageType::Sys,
            PackageType::Web,
            PackageType::DesignPlugins,
            PackageType::DesignSoft,
            PackageType::Model,
        ] {
            let value = init_manifest(pkg_type, "demo", "0.1.0", true);
            let key = if pkg_type == PackageType::Sys {
                "platform-exports"
            } else {
                "exports"
            };
            assert_eq!(value[key], json!({}), "{pkg_type:?} 应生成空 {key}");
            // 不再猜测 `.exe` / `target/release` 之类的路径。
            assert!(
                value[key]
                    .as_object()
                    .is_some_and(|object| object.is_empty()),
                "{pkg_type:?} 的 {key} 不应包含自动路径"
            );
        }
    }

    #[test]
    fn design_plugin_template_has_required_fields() {
        let value = init_manifest(PackageType::DesignPlugins, "demo", "0.1.0", true);
        assert_eq!(value["plugins_type"], "plugins");
        assert_eq!(value["targets"], json!(["common"]));
        assert_eq!(value["use_ai"], false);
        assert_eq!(value["mcp"], "mcp.json");
        assert_eq!(value["exports"], json!({}));
    }

    #[test]
    fn manifest_only_template_has_no_exports() {
        let value = init_manifest(PackageType::App, "demo", "0.1.0", false);
        assert!(value.get("exports").is_none());
        assert!(value.get("platform-exports").is_none());
        let config: DytesConfig = serde_json::from_value(value).unwrap();
        config.validate_with_options(true).unwrap();
        assert!(config.validate().is_err());
        assert_eq!(config.package_type().unwrap(), PackageType::App);
    }
}
