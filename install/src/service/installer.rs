use crate::app::InstallerApp;

pub fn start_install(app: &mut InstallerApp) {
    println!("开始安装到: {}", app.install_path);
    // 这里可以慢慢添加异步下载/解压逻辑
}

