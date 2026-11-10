//! Slint 图形安装器（`gui` feature）。
//!
//! `dytes install --gui` 启动该窗口；安装逻辑复用
//! [`crate::install::installer::run_install`]，界面只负责目录选择与进度展示。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use slint::ComponentHandle;

use crate::install::installer::{self, InstallProgress};

slint::include_modules!();

/// 启动图形安装向导。
///
/// `install_dir` 为空时使用平台默认安装目录；`launch` 为真时安装完成后启动应用。
pub fn run_install_gui(install_dir: Option<PathBuf>, launch: bool) -> Result<()> {
    let ui = MainWindow::new().context("创建安装窗口失败")?;

    let initial_dir =
        install_dir.unwrap_or_else(|| PathBuf::from(crate::install::dirs::get_default_install_path()));
    ui.set_install_path(initial_dir.to_string_lossy().to_string().into());

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

    // 开始安装：后台线程执行，进度回传到 UI 线程
    let weak = ui.as_weak();
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
        ui.set_page_idx(2);
        ui.set_total_progress(0.0);
        ui.set_file_progress(0.0);
        ui.set_download_status("准备安装...".into());

        let ui_weak = ui.as_weak();
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
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.set_total_progress(1.0);
                            ui.set_file_progress(1.0);
                            ui.set_download_status("安装完成".into());
                            ui.set_page_idx(3);
                        }
                    });
                    if launch {
                        if let Err(error) = installer::launch_app(&app_path) {
                            eprintln!("启动应用失败: {error:#}");
                        }
                    }
                }
                Err(error) => {
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
