use crate::utils::dytes::PackageType;

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
}
