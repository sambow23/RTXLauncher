use eframe::{egui, App};
use rtxlauncher_core::{is_elevated, SettingsStore, JobProgress, AppSettings, detect_gmod_install_folder, launch_game, GitHubRelease};

pub const DEFAULT_IGNORE_PATTERNS: &str = r#"
# 32bit Bridge
bin/.trex/*
bin/d3d8to9.dll
bin/d3d9.dll
bin/LICENSE.txt
bin/NvRemixLauncher32.exe
bin/ThirdPartyLicenses-bridge.txt
bin/ThirdPartyLicenses-d3d8to9.txt
bin/ThirdPartyLicenses-dxvk.txt

# Remix in 64 install
bin/win64/usd/*
bin/win64/artifacts_readme.txt
bin/win64/cudart64_12.dll
bin/win64/d3d9.dll
bin/win64/d3d9.pdb
bin/win64/GFSDK_Aftermath_Lib.x64.dll
bin/win64/NRC_Vulkan.dll
bin/win64/NRD.dll
bin/win64/NvLowLatencyVk.dll
bin/win64/nvngx_dlss.dll
bin/win64/nvngx_dlssd.dll
bin/win64/nvngx_dlssg.dll
bin/win64/NvRemixBridge.exe
bin/win64/nvrtc64_120_0.dll
bin/win64/nvrtc-builtins64_125.dll
bin/win64/rtxio.dll
bin/win64/tbb.dll
bin/win64/tbbmalloc.dll
bin/win64/usd_ms.dll
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab { Install, Mount, Repositories, Settings, About, Logs }

pub struct Toast { pub msg: String, pub color: egui::Color32, pub until: std::time::Instant }

pub struct LauncherApp {
	pub log: String,
	pub progress: u8,
	pub not_elevated_warned: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
	pub settings_store: SettingsStore,
	pub settings: AppSettings,
	pub selected: Tab,
	pub is_running: bool,
	pub show_error_modal: Option<String>,
	pub toasts: Vec<Toast>,
	pub remix_source_idx: usize,
	pub remix_releases: Vec<GitHubRelease>,
	pub remix_release_idx: usize,
	pub remix_rx: Option<std::sync::mpsc::Receiver<Vec<GitHubRelease>>>,
	pub remix_loading: bool,
	pub fixes_source_idx: usize,
	pub fixes_releases: Vec<GitHubRelease>,
	pub fixes_release_idx: usize,
	pub fixes_rx: Option<std::sync::mpsc::Receiver<Vec<GitHubRelease>>>,
	pub fixes_loading: bool,
	pub patch_source_idx: usize,
	// Update dialog state
	pub show_update_dialog: bool,
	pub update_folder_options: Vec<String>,
	pub update_folder_selected: Vec<bool>,
	// Update preview
	pub update_preview_dirty: bool,
	pub update_preview_count: usize,
	pub update_preview_bytes: u64,
	pub show_reapply_dialog: bool,
	pub reapply_fixes: bool,
	pub reapply_patches: bool,
	// Sub-states for tabs
	pub install: crate::ui::install::InstallState,
	pub mount: crate::ui::mount::MountState,
	pub repositories: crate::ui::repositories::RepositoriesState,
}

impl Default for LauncherApp {
	fn default() -> Self {
		let store = SettingsStore::new().unwrap_or_else(|_| panic!("settings store init failed"));
		let mut settings = store.load().unwrap_or_default();
		if settings.manually_specified_install_path.is_none() {
			if let Some(p) = detect_gmod_install_folder() {
				settings.manually_specified_install_path = Some(p.display().to_string());
				let _ = store.save(&settings);
			}
		}
		Self {
			log: String::new(),
			progress: 0,
			not_elevated_warned: false,
			current_job: None,
			settings_store: store,
			settings,
			selected: Tab::Install,
			is_running: false,
			show_error_modal: None,
			toasts: Vec::new(),
			remix_source_idx: 0,
			remix_releases: Vec::new(),
			remix_release_idx: 0,
			remix_rx: None,
			remix_loading: false,
			fixes_source_idx: 0,
			fixes_releases: Vec::new(),
			fixes_release_idx: 0,
			fixes_rx: None,
			fixes_loading: false,
			patch_source_idx: 0,
			show_update_dialog: false,
			update_folder_options: Vec::new(),
			update_folder_selected: Vec::new(),
			update_preview_dirty: false,
			update_preview_count: 0,
			update_preview_bytes: 0,
			show_reapply_dialog: false,
			reapply_fixes: true,
			reapply_patches: true,
			install: Default::default(),
			mount: Default::default(),
			repositories: Default::default(),
		}
	}
}

impl LauncherApp {
	#[allow(dead_code)]
	pub fn append_log(&mut self, msg: &str) { if !self.log.is_empty() { self.log.push('\n'); } self.log.push_str(msg); }
	pub fn add_toast(&mut self, msg: &str, color: egui::Color32) { self.toasts.push(Toast { msg: msg.to_string(), color, until: std::time::Instant::now() + std::time::Duration::from_secs(4) }); }
	fn draw_toasts(&mut self, ctx: &egui::Context) {
		let now = std::time::Instant::now();
		self.toasts.retain(|t| t.until > now);
		let mut y = 12.0;
		for (i, t) in self.toasts.iter().enumerate() {
			egui::Area::new(egui::Id::new(format!("toast-{i}"))).fixed_pos(egui::pos2(220.0, y)).show(ctx, |ui| { ui.colored_label(t.color, &t.msg); });
			y += 22.0;
		}
	}
}

impl App for LauncherApp {
	fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
		egui_extras::install_image_loaders(ctx);
		let is_focused = ctx.input(|i| i.focused);
		if is_focused { ctx.request_repaint_after(std::time::Duration::from_millis(1000)); }

		egui::SidePanel::left("nav").resizable(true).min_width(160.0).show(ctx, |ui| {
			ui.horizontal(|ui| {
				let image = egui::include_image!("gmodrtx.png");
				ui.add(egui::Image::new(image).fit_to_exact_size(egui::vec2(208.0, 208.0)));
			});
			ui.separator();
			ui.selectable_value(&mut self.selected, Tab::Install, "Install");
			ui.selectable_value(&mut self.selected, Tab::Mount, "Mounting");
			ui.selectable_value(&mut self.selected, Tab::Repositories, "Repositories");
			ui.selectable_value(&mut self.selected, Tab::Settings, "Settings");
			ui.selectable_value(&mut self.selected, Tab::Logs, "Logs");
			ui.selectable_value(&mut self.selected, Tab::About, "About");
			ui.add_space(8.0);
			ui.separator();
			#[cfg(windows)]
			{
				if !is_elevated() {
					ui.colored_label(egui::Color32::YELLOW, "Not elevated: some operations may fail.");
					ui.separator();
				}
			}
			ui.add_space(8.0);
			let remaining = ui.available_size();
			ui.allocate_ui_with_layout(remaining, egui::Layout::bottom_up(egui::Align::Center), |ui| {
				let any_running = self.install.is_running || self.repositories.is_running || self.mount.is_running;
				if ui.add_enabled(!any_running, egui::Button::new("Launch Game")).clicked() {
					if let Ok(exec_dir) = std::env::current_exe().and_then(|p| p.parent().map(|p| p.to_path_buf()).ok_or(std::io::Error::from(std::io::ErrorKind::NotFound))) {
						let root_exe = exec_dir.join("gmod.exe");
						let win64_exe = exec_dir.join("bin").join("win64").join("gmod.exe");
						let exe = if win64_exe.exists() { win64_exe } else if root_exe.exists() { root_exe } else { exec_dir.join("hl2.exe") };
						if launch_game(exe, &self.settings).is_ok() { self.add_toast("Launched game", egui::Color32::LIGHT_GREEN); } else { self.add_toast("Failed to launch game — check Proton path/Steam root in Settings", egui::Color32::RED); }
					}
				}
				ui.add_space(6.0);
				// Show install progress if available
				if self.install.is_running {
					let pct = self.install.progress as f32 / 100.0;
					let width = ui.available_width().min(220.0);
					let bar = egui::ProgressBar::new(pct).text(format!("Install: {}%", self.install.progress));
					ui.add_sized(egui::vec2(width, 18.0), bar);
				}
			});
		});

		egui::CentralPanel::default().show(ctx, |ui| {
			match self.selected {
				Tab::Install => { crate::ui::install::render_install_tab(self, ui); }
				Tab::Mount => { crate::ui::mount::render_mount_tab(self, ui); }
				Tab::Repositories => { crate::ui::repositories::render_repositories_tab(self, ui); }
				Tab::Settings => { crate::ui::settings::render_settings_tab(self, ui, ctx); }
				Tab::Logs => { crate::ui::logs::render_logs_tab(self, ui); }
				Tab::About => { crate::ui::about::render_about_tab(self, ui); }
			}
		});
		self.render_update_dialog(ctx);
		self.render_reapply_dialog(ctx);
		self.render_error_modal(ctx);
		self.draw_toasts(ctx);
	}
}

impl LauncherApp {
	pub fn append_global_log(&mut self, msg: &str) {
		if !self.log.is_empty() {
			self.log.push('\n');
		}
		self.log.push_str(msg);
	}

	pub fn prepare_update_dialog(&mut self) {
		self.update_folder_options.clear();
		self.update_folder_selected.clear();
		let vanilla = self.settings.manually_specified_install_path.clone().or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
		if let Some(v) = vanilla {
			let root = std::path::PathBuf::from(v);
			if let Ok(rd) = std::fs::read_dir(&root) {
				for e in rd.flatten() {
					if e.path().is_dir() {
						let name = e.file_name().to_string_lossy().to_string();
						if ["crashes","logs","temp","update","xenmod"].contains(&name.as_str()) { continue; }
						self.update_folder_options.push(name);
						self.update_folder_selected.push(false);
					}
				}
			}
		}
		self.update_preview_dirty = true;
	}

	pub fn render_update_dialog(&mut self, ctx: &egui::Context) {
		if !self.show_update_dialog { return; }
		egui::Window::new("Update Base Game").collapsible(false).resizable(true).show(ctx, |ui| {
			ui.label("Select folders to copy from the vanilla installation:");
			let mut any = false;
			for (i, label) in self.update_folder_options.iter().enumerate() {
				let mut sel = self.update_folder_selected[i];
				if ui.checkbox(&mut sel, label).changed() { self.update_folder_selected[i] = sel; self.update_preview_dirty = true; }
				any |= sel;
			}
			ui.separator();
			if self.update_preview_dirty { self.recompute_update_preview(); }
			ui.label(format!("Will copy approximately {} item(s), {}", self.update_preview_count, humansize::format_size(self.update_preview_bytes, humansize::BINARY)));
			ui.separator();
			ui.horizontal(|ui| {
				if ui.add_enabled(any && !self.is_running, egui::Button::new("Apply")).clicked() {
					self.show_update_dialog = false;
					self.start_base_update_job();
				}
				if ui.button("Cancel").clicked() { self.show_update_dialog = false; }
			});
		});
	}

	fn start_base_update_job(&mut self) {
		let selected_prefixes: Vec<String> = self.update_folder_options.iter().cloned().zip(self.update_folder_selected.iter().cloned()).filter_map(|(l, s)| if s { Some(l) } else { None }).collect();
		let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
		self.current_job = Some(rx);
		self.is_running = true;
		std::thread::spawn(move || {
			let src = rtxlauncher_core::detect_gmod_install_folder().unwrap_or_default();
			let dst = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
			let updates = rtxlauncher_core::detect_updates(&src, &dst).unwrap_or_default();
			let include_root_execs = selected_prefixes.iter().any(|p| p == "bin");
			let filtered: Vec<_> = updates.into_iter().filter(|u| {
				if selected_prefixes.is_empty() { return false; }
				let rp = &u.relative_path;
				if !rp.contains('/') { return include_root_execs && (rp.eq_ignore_ascii_case("gmod.exe") || rp.eq_ignore_ascii_case("hl2.exe") || rp.eq_ignore_ascii_case("steam_appid.txt")); }
				for p in &selected_prefixes { let prefix = format!("{}/", p); if rp.starts_with(&prefix) || rp == p { return true; } }
				false
			}).collect();
			let _ = rtxlauncher_core::apply_updates(&filtered, |m,p| { let scaled = ((p as u16 * 90) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); });
			let _ = tx.send(JobProgress { message: "Base game update complete".into(), percent: 100 });
		});
		self.show_reapply_dialog = true; self.reapply_fixes = true; self.reapply_patches = true;
	}

	pub fn render_reapply_dialog(&mut self, ctx: &egui::Context) {
		if !self.show_reapply_dialog || self.is_running { return; }
		egui::Window::new("Reapply Components?").collapsible(false).resizable(false).show(ctx, |ui| {
			ui.label("Reapply components after updating base game?");
			ui.checkbox(&mut self.reapply_fixes, "Reapply Fixes Package");
			ui.checkbox(&mut self.reapply_patches, "Reapply Binary Patches");
			ui.horizontal(|ui| {
				if ui.button("Proceed").clicked() { self.show_reapply_dialog = false; self.trigger_reapply_jobs(); }
				if ui.button("Skip").clicked() { self.show_reapply_dialog = false; }
			});
		});
	}

	fn trigger_reapply_jobs(&mut self) {
		if self.reapply_fixes {
			if let Some(rel) = self.repositories.fixes_releases.get(self.repositories.fixes_release_idx).cloned() {
				let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
				self.current_job = Some(rx);
				self.is_running = true;
				std::thread::spawn(move || { let rt = tokio::runtime::Runtime::new().unwrap(); rt.block_on(async move { let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default(); let _ = rtxlauncher_core::install_fixes_from_release(&rel, &base, Some(DEFAULT_IGNORE_PATTERNS), |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await; }); });
			}
		}
		if self.reapply_patches {
			let (owner, repo) = { let s = [("sambow23","SourceRTXTweaks"),("BlueAmulet","SourceRTXTweaks"),("Xenthio","SourceRTXTweaks")][self.repositories.patch_source_idx.min(2)]; (s.0.to_string(), s.1.to_string()) };
			let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
			self.current_job = Some(rx);
			self.is_running = true;
			let install_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
			std::thread::spawn(move || { let rt = tokio::runtime::Runtime::new().unwrap(); rt.block_on(async move { let _ = rtxlauncher_core::apply_patches_from_repo(&owner, &repo, "applypatch.py", &install_dir, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await; }); });
		}
	}

	fn render_error_modal(&mut self, ctx: &egui::Context) {
		if let Some(msg) = self.show_error_modal.clone() {
			egui::Window::new("Error").collapsible(false).resizable(true).show(ctx, |ui| {
				ui.colored_label(egui::Color32::RED, &msg);
				ui.horizontal(|ui| {
					if ui.button("Copy details").clicked() { ui.output_mut(|o| o.copied_text = msg.clone()); self.add_toast("Copied error", egui::Color32::LIGHT_GREEN); }
					if ui.button("Close").clicked() { self.show_error_modal = None; }
				});
			});
		}
	}

	fn recompute_update_preview(&mut self) {
		self.update_preview_dirty = false;
		self.update_preview_count = 0;
		self.update_preview_bytes = 0;
		let vanilla = self.settings.manually_specified_install_path.clone().or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
		let Some(v) = vanilla else { return; };
		let src = std::path::PathBuf::from(v);
		let dst = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
		let updates = rtxlauncher_core::detect_updates(&src, &dst).unwrap_or_default();
		let include_root_execs = self.update_folder_selected.iter().enumerate().any(|(i, s)| *s && self.update_folder_options.get(i).map(|p| p == "bin").unwrap_or(false));
		for u in updates.into_iter() {
			let rp = u.relative_path.clone();
			let mut include = false;
			if !rp.contains('/') { include = include_root_execs && (rp.eq_ignore_ascii_case("gmod.exe") || rp.eq_ignore_ascii_case("hl2.exe") || rp.eq_ignore_ascii_case("steam_appid.txt")); }
			for (i, l) in self.update_folder_options.iter().enumerate() {
				if !self.update_folder_selected[i] { continue; }
				let prefix = format!("{}/", l);
				if rp.starts_with(&prefix) || rp == *l { include = true; break; }
			}
			if include {
				self.update_preview_count += 1;
				if !u.is_directory { if let Ok(meta) = std::fs::metadata(&u.source_path) { self.update_preview_bytes = self.update_preview_bytes.saturating_add(meta.len()); } }
			}
		}
	}
}


