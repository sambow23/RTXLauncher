use eframe::egui;
use rtxlauncher_core::{mount_game, unmount_game, JobProgress, apply_usda_fixes};

pub struct MountState {
	pub mount_game_folder: String,
	pub mount_remix_mod: String,
	pub is_running: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
}

impl Default for MountState {
	fn default() -> Self {
		Self { mount_game_folder: "hl2rtx".to_string(), mount_remix_mod: "hl2rtx".to_string(), is_running: false, current_job: None }
	}
}

impl MountState {
	pub fn poll_job(&mut self, global_log: &mut String) {
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				// Append to global log
				if !global_log.is_empty() { global_log.push('\n'); }
				global_log.push_str(&p.message);
				if p.percent >= 100 { self.is_running = false; }
			}
			if self.is_running { self.current_job = Some(rx); }
		}
	}
}

pub fn render_mount_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	{
		let st = &mut app.mount;
		st.poll_job(&mut app.log);
	}
	ui.heading("Mounting");
	ui.add_enabled_ui(!app.mount.is_running, |ui| {
		ui.label("Detected mountable games:");
		let mut detected: Vec<(&'static str, Option<std::path::PathBuf>, &'static str)> = vec![
			("Half-Life 2 RTX", rtxlauncher_core::detect_install_folder_path("Half-Life 2 RTX"), "hl2rtx"),
			("Portal RTX", rtxlauncher_core::detect_install_folder_path("Portal RTX"), "portalrtx"),
		];
		for (name, path_opt, mod_folder) in detected.drain(..) {
			let label = if let Some(p) = path_opt { format!("{} — {}", name, p.display()) } else { format!("{} — not found", name) };
			if ui.button(label).clicked() {
				app.mount.mount_game_folder = mod_folder.to_string();
				app.mount.mount_remix_mod = mod_folder.to_string();
			}
		}
		ui.separator();
		let mut gf = app.mount.mount_game_folder.clone();
		ui.horizontal(|ui| { ui.label("Game folder (source content):"); ui.text_edit_singleline(&mut gf); });
		app.mount.mount_game_folder = gf;
		let mut rm = app.mount.mount_remix_mod.clone();
		ui.horizontal(|ui| { ui.label("Remix mod folder:"); ui.text_edit_singleline(&mut rm); });
		app.mount.mount_remix_mod = rm;
		// Mounted status
		let mounted = rtxlauncher_core::is_game_mounted(&app.mount.mount_game_folder, "Half-Life 2 RTX", &app.mount.mount_remix_mod);
		let status_col = if mounted { egui::Color32::from_rgb(0,200,0) } else { egui::Color32::from_rgb(200,0,0) };
		ui.colored_label(status_col, if mounted { "Mounted" } else { "Not mounted" });
		if ui.button("Mount").clicked() {
			let gf = app.mount.mount_game_folder.clone();
			let rm = app.mount.mount_remix_mod.clone();
			let mut tmp = String::new();
			let _ = mount_game(&gf, "Half-Life 2 RTX", &rm, |m| { tmp.push_str(m); tmp.push('\n'); });
			app.append_global_log(&tmp);
		}
		if ui.button("Unmount").clicked() {
			let gf = app.mount.mount_game_folder.clone();
			let rm = app.mount.mount_remix_mod.clone();
			let mut tmp = String::new();
			let _ = unmount_game(&gf, "Half-Life 2 RTX", &rm, |m| { tmp.push_str(m); tmp.push('\n'); });
			app.append_global_log(&tmp);
		}
		ui.separator();
		if ui.button("Apply USDA fixes for hl2rtx").clicked() {
			let (tx, rx) = std::sync::mpsc::channel::<rtxlauncher_core::JobProgress>();
			app.mount.current_job = Some(rx);
			app.mount.is_running = true;
			std::thread::spawn(move || {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async move {
					let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
					let _ = apply_usda_fixes(&base, "hl2rtx", |m,p| { let _ = tx.send(rtxlauncher_core::JobProgress { message: m.to_string(), percent: p }); }).await;
				});
			});
		}
	});
}


