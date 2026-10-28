use crate::shared::config::PackageType;
use serde_json::{json, Value};

pub fn init_manifest(pkg_type: PackageType, name: &str, version: &str) -> Value {
    let base = json!({
        "name": name,
        "version": version,
        "description": format!("{name} package"),
        "author": "funxdata",
        "type": pkg_type.as_str(),
    });
    let mut manifest = base;
    let object = manifest.as_object_mut().expect("manifest object");
    match pkg_type {
        PackageType::App | PackageType::Core => {
            object.insert(
                "exports".to_string(),
                json!({ format!("./target/release/{name}.exe"): format!("{name}.exe") }),
            );
        }
        PackageType::Sys => {
            object.insert(
                "platform-exports".to_string(),
                json!({
                    "windows": {
                        format!("./target/release/{name}.dll"): format!("libary/{name}.dll")
                    }
                }),
            );
        }
        PackageType::Web => {
            object.insert(
                "exports".to_string(),
                json!({
                    format!("./assets/{name}.js"): format!("{name}.js"),
                    "./src/setup.ts": "setup.ts"
                }),
            );
            object.insert("init".to_string(), json!(["./setup.ts"]));
        }
        PackageType::DesignPlugins => {
            object.insert(
                "exports".to_string(),
                json!({
                    format!("./target/release/{name}.dll"): format!("{name}.dll"),
                    "./mcp.json": "mcp.json"
                }),
            );
            object.insert("targets".to_string(), json!([]));
            object.insert("use_ai".to_string(), json!(false));
        }
        PackageType::DesignSoft => {
            object.insert(
                "exports".to_string(),
                json!({ format!("./target/release/{name}.exe"): format!("{name}.exe") }),
            );
        }
        PackageType::Model => {
            object.insert(
                "exports".to_string(),
                json!({ format!("./{name}.gguf"): format!("{name}.gguf") }),
            );
        }
    }
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::config::DytesConfig;

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
            let value = init_manifest(pkg_type, "demo", "0.1.0");
            let config: DytesConfig = serde_json::from_value(value).unwrap();
            config.validate().unwrap();
            assert_eq!(config.package_type().unwrap(), pkg_type);
        }
    }
}
