
pub mod utils;
pub mod run;

// 重新导出常用的类型和函数，方便外部使用
pub use utils::version::{check_version, compare_versions};
// pub use utils::env::{check_network, get_app_dir, get_temp_dir, get_exe_path};
// pub use utils::loader::{download_file, fetch_text, fetch_json};
pub use run::update_app::update_app_info;
pub use run::dytes::init_dytes;