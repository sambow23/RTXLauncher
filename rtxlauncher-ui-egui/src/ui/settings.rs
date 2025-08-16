use eframe::egui;
use rtxlauncher_core::{detect_gmod_install_folder, is_elevated};

pub struct SettingsState {}

impl Default for SettingsState { fn default() -> Self { Self {} } }

pub fn render_settings_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui, ctx: &egui::Context) {
	ui.heading("Settings");
	let mut path_display = app.settings.manually_specified_install_path.clone().unwrap_or_default();
	ui.horizontal(|ui| {
		ui.label("Original Garry's Mod path:");
		ui.text_edit_singleline(&mut path_display);
		if ui.add_enabled(!app.install.is_running, egui::Button::new("Browse")).clicked() {
			if let Some(p) = rfd::FileDialog::new().set_directory("C:/").pick_folder() {
				app.settings.manually_specified_install_path = Some(p.display().to_string());
				let _ = app.settings_store.save(&app.settings);
			}
		}
		if ui.add_enabled(!app.install.is_running, egui::Button::new("Auto-detect (Steam)")).clicked() {
			if let Some(p) = detect_gmod_install_folder() {
				app.settings.manually_specified_install_path = Some(p.display().to_string());
				let _ = app.settings_store.save(&app.settings);
			}
		}
	});
    // Path validation hint
    let path_ok = app.settings.manually_specified_install_path.as_ref().map(|p| std::path::Path::new(p).exists()).unwrap_or(false)
        || detect_gmod_install_folder().is_some();
    let col = if path_ok { egui::Color32::from_rgb(0,200,0) } else { egui::Color32::from_rgb(200,0,0) };
    ui.colored_label(col, if path_ok { "GMod path OK" } else { "GMod path not found" });
	ui.horizontal(|ui| {
		ui.label("GitHub PAT (optional):");
		let mut pat = rtxlauncher_core::load_personal_access_token().unwrap_or_default();
		ui.add(egui::TextEdit::singleline(&mut pat).password(true).desired_width(200.0));
		if ui.button("Save PAT").clicked() {
			let _ = rtxlauncher_core::set_personal_access_token(if pat.trim().is_empty() { None } else { Some(pat.clone()) });
		}
	});
    // PAT validation hint
    let pat_ok = rtxlauncher_core::load_personal_access_token().map(|s| !s.is_empty()).unwrap_or(false);
    let col = if pat_ok { egui::Color32::from_rgb(0,200,0) } else { egui::Color32::from_rgb(200,0,0) };
    ui.colored_label(col, if pat_ok { "PAT saved" } else { "No PAT" });
	ui.separator();
	ui.label("Launch options");
	// Resolution dropdown
	let mut resolutions: Vec<(u32, u32)> = vec![(1280,720),(1280,800),(1366,768),(1440,900),(1600,900),(1680,1050),(1920,1080),(1920,1200),(2560,1080),(2560,1440),(2560,1600),(3440,1440),(3840,2160)];
	let win_size = ctx.input(|i| i.screen_rect.size());
	let current_px = (win_size.x.round() as u32, win_size.y.round() as u32);
	if current_px.0 > 0 && current_px.1 > 0 && !resolutions.contains(&current_px) { resolutions.insert(0, current_px); }
	resolutions.dedup();
	ui.horizontal(|ui| {
		ui.label("Resolution:");
		let sel_w = app.settings.width.unwrap_or(0);
		let sel_h = app.settings.height.unwrap_or(0);
		let is_custom = !(sel_w > 0 && sel_h > 0 && resolutions.contains(&(sel_w, sel_h)));
		let selected_text = if is_custom { "Custom".to_string() } else { format!("{}x{}", sel_w, sel_h) };
		egui::ComboBox::from_id_salt("res-dropdown").selected_text(selected_text).show_ui(ui, |ui| {
			if ui.selectable_label(is_custom, "Custom").clicked() {
				app.settings.width = None; app.settings.height = None; let _ = app.settings_store.save(&app.settings);
			}
			for (w,h) in resolutions.iter().cloned() {
				let label = format!("{}x{}", w,h);
				let is_sel = sel_w==w && sel_h==h;
				if ui.selectable_label(is_sel, label).clicked() {
					app.settings.width = Some(w); app.settings.height = Some(h); let _ = app.settings_store.save(&app.settings);
				}
			}
		});
	});
	let sel_w2 = app.settings.width.unwrap_or(0);
	let sel_h2 = app.settings.height.unwrap_or(0);
	let is_custom2 = !(sel_w2 > 0 && sel_h2 > 0 && resolutions.contains(&(sel_w2, sel_h2)));
	if is_custom2 {
		ui.horizontal(|ui| {
			let mut w = app.settings.width.unwrap_or_default();
			ui.label("Width");
			if ui.add(egui::DragValue::new(&mut w).range(0..=16384)).changed() { app.settings.width = Some(w); let _ = app.settings_store.save(&app.settings); }
			let mut h = app.settings.height.unwrap_or_default();
			ui.label("Height");
			if ui.add(egui::DragValue::new(&mut h).range(0..=16384)).changed() { app.settings.height = Some(h); let _ = app.settings_store.save(&app.settings); }
		});
	}
	if ui.checkbox(&mut app.settings.console_enabled, "Enable console").changed() { let _ = app.settings_store.save(&app.settings); }
	if ui.checkbox(&mut app.settings.load_workshop_addons, "Load Workshop Addons").changed() { let _ = app.settings_store.save(&app.settings); }
	if ui.checkbox(&mut app.settings.disable_chromium, "Disable Chromium").changed() { let _ = app.settings_store.save(&app.settings); }
	if ui.checkbox(&mut app.settings.developer_mode, "Developer mode").changed() { let _ = app.settings_store.save(&app.settings); }
	if ui.checkbox(&mut app.settings.tools_mode, "Particle Editor Mode").changed() { let _ = app.settings_store.save(&app.settings); }
	ui.horizontal(|ui| { ui.label("Custom args:"); let mut custom = app.settings.custom_launch_options.clone().unwrap_or_default(); if ui.text_edit_singleline(&mut custom).changed() { app.settings.custom_launch_options = if custom.trim().is_empty() { None } else { Some(custom) }; let _ = app.settings_store.save(&app.settings); } });

	#[cfg(windows)]
	{
		if !is_elevated() {
			if ui.button("Relaunch as Administrator").clicked() {
				let exe = std::env::current_exe().ok();
				if let Some(exe) = exe {
					use windows::Win32::{UI::Shell::ShellExecuteW, Foundation::HWND};
					use windows::core::PCWSTR;
					use std::os::windows::ffi::OsStrExt;
					let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
					unsafe {
						let _ = ShellExecuteW(
							HWND(std::ptr::null_mut()),
							PCWSTR("runas\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
							PCWSTR(wide.as_ptr()),
							PCWSTR(std::ptr::null()),
							PCWSTR(std::ptr::null()),
							windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
						);
					}
				}
			}
		}
	}
}


