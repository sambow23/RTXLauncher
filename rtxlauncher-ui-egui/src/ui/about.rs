use eframe::egui;

pub fn render_about_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	ui.heading("About");
	ui.separator();
	ui.label("A recreation of Xenthio's original .NET launcher, aimed for cross-platform support like Linux, in addition to upcoming features.");
	ui.separator();
	let git = option_env!("GIT_COMMIT_HASH").unwrap_or("unknown");
	ui.label(format!("Launcher version: {}", git));
	if let Some(p) = rtxlauncher_core::detect_gmod_install_folder() {
		if let Ok(meta) = std::fs::metadata(&p) {
			if let Ok(modified) = meta.modified() {
				use chrono::{DateTime, Local};
				let dt: DateTime<Local> = modified.into();
				ui.label(format!("GMod install modified: {}", dt.format("%d/%m/%Y %H:%M")));
			}
		}
	}
	let remix_v = app.settings.installed_remix_version.clone().unwrap_or_else(|| "(unknown)".into());
	let fixes_v = app.settings.installed_fixes_version.clone().unwrap_or_else(|| "(unknown)".into());
	let patch_c = app.settings.installed_patches_commit.clone().unwrap_or_else(|| "(none)".into());
	ui.label(format!("Installed Remix: {}", remix_v));
	ui.label(format!("Installed Fixes: {}", fixes_v));
	ui.label(format!("Applied Patches: {}", patch_c));
}


