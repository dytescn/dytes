use crate::shared::config::PackageType;

/// 发布后的远程目录前缀。
pub fn remote_prefix(pkg_type: PackageType) -> &'static str {
    match pkg_type {
        PackageType::App => "app",
        PackageType::Core => "core",
        PackageType::Sys => "system",
        PackageType::Web => "webs",
        PackageType::DesignPlugins => "design_plugins",
        PackageType::DesignSoft => "design_soft",
        PackageType::Model => "models",
    }
}

/// 索引文件路径。
pub fn packages_index(pkg_type: PackageType) -> String {
    format!("{}/packages.json", remote_prefix(pkg_type))
}

/// 安装 / 解包目标（相对安装根目录）。
pub fn install_dir(pkg_type: PackageType, name: &str) -> Option<String> {
    match pkg_type {
        PackageType::App | PackageType::Core => None,
        PackageType::Sys => Some(format!("libary/{name}.dll")),
        PackageType::Web => Some(format!("web/{name}")),
        PackageType::DesignPlugins => Some(format!("plugins/{name}")),
        PackageType::DesignSoft => Some(format!("apps/{name}")),
        PackageType::Model => Some(format!("models/{name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_remote_prefixes() {
        assert_eq!(remote_prefix(PackageType::Sys), "system");
        assert_eq!(remote_prefix(PackageType::Web), "webs");
        assert_eq!(
            packages_index(PackageType::DesignPlugins),
            "design_plugins/packages.json"
        );
    }

    #[test]
    fn maps_install_dirs() {
        assert_eq!(
            install_dir(PackageType::Web, "setting").as_deref(),
            Some("web/setting")
        );
        assert_eq!(install_dir(PackageType::App, "piksel"), None);
    }
}
