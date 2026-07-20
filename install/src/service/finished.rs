fn finished(app: &mut InstallerApp, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);

        ui.heading("安装完成 🎉");

        ui.add_space(20.0);

        ui.label("程序已成功安装");

        ui.add_space(20.0);

        if ui.button("关闭").clicked() {
            std::process::exit(0);
        }

        if ui.button("返回首页").clicked() {
            app.page = Page::Welcome;
        }
    });
}
