//! Slint 图形安装器（`gui` feature）。
//!
//! `dytes install --gui` 启动该窗口；安装逻辑复用
//! [`crate::install::installer::run_install`]，界面只负责目录选择与进度展示。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use slint::ComponentHandle;

use crate::install::installer::{self, InstallProgress};

// build.rs 分别编译 install.slint / update.slint，这里同时引入两个生成模块。
include!(concat!(env!("OUT_DIR"), "/install.rs"));
include!(concat!(env!("OUT_DIR"), "/update.rs"));

/// 启动图形安装向导。
///
/// `install_dir` 为空时使用平台默认安装目录；`launch` 为真时安装完成后启动应用。
pub fn run_install_gui(install_dir: Option<PathBuf>, launch: bool) -> Result<()> {
    let ui = MainWindow::new().context("创建安装窗口失败")?;
    eprintln!("[gui-debug] launch flag = {launch}");

    let initial_dir =
        install_dir.unwrap_or_else(|| PathBuf::from(crate::install::dirs::get_default_install_path()));
    ui.set_install_path(initial_dir.to_string_lossy().to_string().into());

    // 安装成功后记录应用路径，供完成页的“启动”按钮使用。
    let installed_app: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

    // 选择安装目录
    let weak = ui.as_weak();
    ui.on_open_browse_dialog(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let current = Path::new(ui.get_install_path().as_str()).to_path_buf();
        let mut dialog = rfd::FileDialog::new();
        if current.is_dir() {
            dialog = dialog.set_directory(&current);
        }
        if let Some(folder) = dialog.pick_folder() {
            ui.set_install_path(folder.to_string_lossy().to_string().into());
        }
    });

    // 完成页：启动已安装的应用，随后关闭安装窗口
    let weak = ui.as_weak();
    let installed = installed_app.clone();
    ui.on_launch_app(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let app_path = installed.lock().ok().and_then(|path| path.clone());
        let Some(app_path) = app_path else {
            ui.set_launch_error("尚未完成安装，无法启动应用".into());
            return;
        };
        match installer::launch_app(&app_path) {
            Ok(()) => close_installer(&ui),
            Err(error) => {
                ui.set_launch_error(format!("启动应用失败: {error:#}").into());
            }
        }
    });

    // 完成页：仅关闭安装窗口
    let weak = ui.as_weak();
    ui.on_close_window(move || {
        if let Some(ui) = weak.upgrade() {
            close_installer(&ui);
        }
    });

    // 开始安装：后台线程执行，进度回传到 UI 线程
    let weak = ui.as_weak();
    let installed = installed_app.clone();
    ui.on_start_install(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };

        let install_dir_text = ui.get_install_path().trim().to_string();
        if install_dir_text.is_empty() {
            ui.set_install_failed(true);
            ui.set_download_status("请先选择安装目录".into());
            ui.set_page_idx(2);
            return;
        }
        let install_dir = PathBuf::from(install_dir_text);

        ui.set_install_failed(false);
        ui.set_launch_error("".into());
        ui.set_page_idx(2);
        ui.set_total_progress(0.0);
        ui.set_file_progress(0.0);
        ui.set_download_status("准备安装...".into());
        if let Ok(mut app) = installed.lock() {
            *app = None;
        }

        let ui_weak = ui.as_weak();
        let installed = installed.clone();
        std::thread::spawn(move || {
            let progress_weak = ui_weak.clone();
            let report = move |progress: InstallProgress| {
                let weak = progress_weak.clone();
                let status = progress.status.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_total_progress(progress.total);
                        ui.set_file_progress(progress.file);
                        ui.set_download_status(status.into());
                    }
                });
            };

            match installer::run_install(&install_dir, report) {
                Ok(app_path) => {
                    if let Ok(mut app) = installed.lock() {
                        *app = Some(app_path.clone());
                    }
                    let auto_launch = launch;
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_total_progress(1.0);
                            ui.set_file_progress(1.0);
                            ui.set_download_status("安装完成".into());
                            ui.set_page_idx(3);
                            // `--launch`：安装完成后直接启动并退出安装器。
                            if auto_launch {
                                match installer::launch_app(&app_path) {
                                    Ok(()) => close_installer(&ui),
                                    Err(error) => ui
                                        .set_launch_error(format!("启动应用失败: {error:#}").into()),
                                }
                            }
                        }
                    });
                }
                Err(error) => {
                    if let Ok(mut app) = installed.lock() {
                        *app = None;
                    }
                    let message = format!("安装失败: {error:#}");
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_install_failed(true);
                            ui.set_download_status(message.into());
                        }
                    });
                }
            }
        });
    });

    ui.run().context("运行安装窗口失败")?;
    Ok(())
}

/// 关闭安装窗口并结束事件循环，避免安装器在启动应用后继续停留。
fn close_installer(ui: &MainWindow) {
    let _ = ui.hide();
    let _ = slint::quit_event_loop();
}

/// 启动图形更新窗口。
///
/// `dir` 为空时使用当前目录；点击“开始更新”后通过内置更新引擎执行。
pub fn run_update_gui(dir: Option<PathBuf>) -> Result<()> {
    let ui = UpdatePage::new().context("创建更新窗口失败")?;
    ui.set_status_text("点击按钮检查并安装更新".into());

    // 窗口打开即延后下次更新检测，避免用户忽略窗口时反复检测。
    let base = dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let next = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
        + crate::update::dytes::UPDATE_CHECK_INTERVAL_SECS;
    if let Err(error) = crate::update::dytes::set_update_check_after(&base, next) {
        eprintln!("写入下次更新检测时间失败: {error:#}");
    }

    let weak = ui.as_weak();
    let update_dir = dir.clone();
    ui.on_update_clicked(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        ui.set_show_progress(true);
        ui.set_show_restart(false);
        ui.set_progress_value(0.0);
        ui.set_status_text("准备更新...".into());

        let ui_weak = ui.as_weak();
        let dir = update_dir.clone();
        std::thread::spawn(move || {
            let previous = std::env::current_dir().ok();
            if let Some(dir) = &dir {
                if let Err(error) = std::env::set_current_dir(dir) {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_show_progress(false);
                            ui.set_status_text(format!("切换目录失败: {error}").into());
                        }
                    });
                    return;
                }
            }

            let progress_weak = ui_weak.clone();
            let progress: crate::update::dytes::ProgressCallback =
                Box::new(move |percent, status| {
                    let weak = progress_weak.clone();
                    let status = status.to_string();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.set_progress_value(percent.clamp(0.0, 100.0));
                            ui.set_status_text(status.into());
                        }
                    });
                });

            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime.block_on(crate::update::dytes::init_dytes_with_options(
                    Some(progress),
                    true,
                )),
                Err(error) => Err(anyhow::anyhow!("创建异步运行时失败: {error}")),
            };

            if let Some(previous) = previous {
                let _ = std::env::set_current_dir(previous);
            }

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    match result {
                        Ok(()) => {
                            ui.set_show_progress(false);
                            ui.set_show_restart(true);
                            ui.set_progress_value(100.0);
                            ui.set_status_text("更新完成，请重启应用".into());
                        }
                        Err(error) => {
                            ui.set_show_progress(false);
                            ui.set_show_restart(false);
                            ui.set_status_text(format!("更新失败: {error:#}").into());
                        }
                    }
                }
            });
        });
    });

    let weak = ui.as_weak();
    let restart_dir = dir;
    ui.on_restart_clicked(move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let base = restart_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        if let Ok(config) = crate::utils::dytes::DytesConfig::read(&base.join("dytes.json")) {
            let exe = if cfg!(windows) && !config.name.to_lowercase().ends_with(".exe") {
                format!("{}.exe", config.name)
            } else {
                config.name.clone()
            };
            if let Err(error) = crate::install::installer::launch_app(&base.join(exe)) {
                ui.set_status_text(format!("启动应用失败: {error:#}").into());
                return;
            }
        }
        let _ = ui.hide();
        let _ = slint::quit_event_loop();
    });

    ui.run().context("运行更新窗口失败")?;
    Ok(())
}