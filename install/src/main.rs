pub mod utils;
slint::include_modules!();
use utils::dirs::get_default_install_path;

fn main() -> Result<(), slint::PlatformError> {
    let main_win = MainWindow::new()?;
    let win_weak = main_win.as_weak();

    // 预先克隆两份，各自独占一份，互不抢夺所有权
    let weak_browse = win_weak.clone();
    let weak_install = win_weak.clone();

    // 自动设置默认路径
    let default_path = get_default_install_path();
    main_win.set_install_path(default_path.into());

    // 选择文件夹
    main_win.on_open_browse_dialog(move || {
        let ui_weak = weak_browse.clone();
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            let path_str = folder.to_string_lossy().to_string();
            slint::invoke_from_event_loop(move || {
                let ui = ui_weak.unwrap();
                ui.set_install_path(path_str.into());
            });
        }
    });

    // 开始安装
    main_win.on_start_install(move || {
        let thread_weak = weak_install.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let invoke_weak = thread_weak.clone();
            slint::invoke_from_event_loop(move || {
                let ui = invoke_weak.unwrap();
                ui.set_page_idx(3);
            });
        });
    });

    main_win.run()
}
