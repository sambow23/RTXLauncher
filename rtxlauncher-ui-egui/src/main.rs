#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::{egui, App};
use rtxlauncher_core::{is_elevated, SettingsStore, JobProgress, AppSettings, detect_gmod_install_folder, InstallPlan, perform_basic_install, mount_game, unmount_game, fetch_releases, GitHubRateLimit, install_remix_from_release, install_fixes_from_release, apply_usda_fixes, detect_updates, apply_updates, launch_game, set_personal_access_token, load_personal_access_token, init_logging, GitHubRelease, apply_patches_from_repo};
#[cfg(unix)]
use rtxlauncher_core::launch::list_proton_builds;
use std::sync::mpsc::Receiver;
use rfd::FileDialog;
#[cfg(windows)]
use windows::Win32::{UI::Shell::ShellExecuteW, Foundation::HWND};
#[cfg(windows)]
use windows::core::PCWSTR;

const DEFAULT_IGNORE_PATTERNS: &str = r#"
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
enum Tab { Install, Mount, Repositories, Settings, About }

struct Toast { msg: String, color: egui::Color32, until: std::time::Instant }

struct LauncherApp {
	log: String,
	progress: u8,
	not_elevated_warned: bool,
	current_job: Option<Receiver<JobProgress>>,
	settings_store: SettingsStore,
	settings: AppSettings,
	selected: Tab,
	is_running: bool,
	mount_game_folder: String,
	mount_remix_mod: String,
	toasts: Vec<Toast>,
	// Repositories tab state
	remix_source_idx: usize,
	remix_releases: Vec<GitHubRelease>,
	remix_release_idx: usize,
	remix_rx: Option<std::sync::mpsc::Receiver<Vec<GitHubRelease>>>,
	remix_loading: bool,
	fixes_source_idx: usize,
	fixes_releases: Vec<GitHubRelease>,
	fixes_release_idx: usize,
	fixes_rx: Option<std::sync::mpsc::Receiver<Vec<GitHubRelease>>>,
	fixes_loading: bool,
	patch_source_idx: usize,
	// Update Base Game dialog state
	show_update_dialog: bool,
	update_folder_options: Vec<String>,
	update_folder_selected: Vec<bool>,
	show_reapply_dialog: bool,
	reapply_fixes: bool,
	reapply_patches: bool,
}

impl Default for LauncherApp {
	fn default() -> Self {
		let store = SettingsStore::new().unwrap_or_else(|_| panic!("settings store init failed"));
		let mut settings = store.load().unwrap_or_default();
		// Auto-detect Steam install path on first run if not set, so the UI isn't empty
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
			mount_game_folder: "hl2rtx".to_string(),
			mount_remix_mod: "hl2rtx".to_string(),
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
			show_reapply_dialog: false,
			reapply_fixes: true,
			reapply_patches: true,
		}
	}
}

impl App for LauncherApp {
	fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
		// Ensure image loaders are installed once per frame; cheap no-op after first
		egui_extras::install_image_loaders(ctx);
		// Ensure UI refreshes even without input so progress/logs update
		// Only repaint periodically while the window is focused to reduce background CPU usage
		let is_focused = ctx.input(|i| i.focused);
		if is_focused {
			ctx.request_repaint_after(std::time::Duration::from_millis(1000));
		}

		egui::SidePanel::left("nav").resizable(true).min_width(160.0).show(ctx, |ui| {
			// Icon + title
			ui.horizontal(|ui| {
				let image = egui::include_image!("gmodrtx.png");
				ui.add(egui::Image::new(image).fit_to_exact_size(egui::vec2(208.0, 208.0)));
			});
			ui.separator();
			ui.selectable_value(&mut self.selected, Tab::Install, "Install");
			ui.selectable_value(&mut self.selected, Tab::Mount, "Mounting");
			ui.selectable_value(&mut self.selected, Tab::Repositories, "Repositories");
			ui.selectable_value(&mut self.selected, Tab::Settings, "Settings");
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
				// Bottom-most: Launch button
				if ui.add_enabled(!self.is_running, egui::Button::new("Launch Game")).clicked() {
					// Always launch relative to the launcher location to avoid starting vanilla by accident
					if let Ok(exec_dir) = std::env::current_exe().and_then(|p| p.parent().map(|p| p.to_path_buf()).ok_or(std::io::Error::from(std::io::ErrorKind::NotFound))) {
						let root_exe = exec_dir.join("gmod.exe");
						let win64_exe = exec_dir.join("bin").join("win64").join("gmod.exe");
						let exe = if win64_exe.exists() { win64_exe } else if root_exe.exists() { root_exe } else { exec_dir.join("hl2.exe") };
						if launch_game(exe, &self.settings).is_ok() { self.add_toast("Launched game", egui::Color32::LIGHT_GREEN); } else { self.add_toast("Failed to launch game — check Proton path/Steam root in Settings", egui::Color32::RED); }
					}
				}
				ui.add_space(6.0);
				if self.is_running {
					if ui.button("Cancel").clicked() {
						self.current_job = None;
						self.is_running = false;
						self.add_toast("Job cancelled", egui::Color32::YELLOW);
					}
					ui.add_space(6.0);
					let pct = self.progress as f32 / 100.0;
					let width = ui.available_width().min(220.0);
					let bar = egui::ProgressBar::new(pct).text(format!("Progress: {}%", self.progress));
					ui.add_sized(egui::vec2(width, 18.0), bar);
				}
			});
		});

		egui::CentralPanel::default().show(ctx, |ui| {
			self.poll_job();
			ui.separator();
			match self.selected {
				Tab::Install => {
					ui.heading("Install");
					ui.add_enabled_ui(!self.is_running, |ui| {
						if ui.button("Quick Install").clicked() {
							let vanilla_opt = self.settings.manually_specified_install_path.clone().or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
							if let Some(vanilla) = vanilla_opt {
								if let Ok(exec_dir) = std::env::current_exe().map(|p| p.parent().unwrap().to_path_buf()) {
									let plan = InstallPlan { vanilla: std::path::PathBuf::from(vanilla), rtx: exec_dir.clone() };
									let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
									self.current_job = Some(rx);
									self.is_running = true;
									// capture current repo selections
									let remix_source_idx = self.remix_source_idx;
									let remix_release_idx = self.remix_release_idx;
									let fixes_source_idx = self.fixes_source_idx;
									let fixes_release_idx = self.fixes_release_idx;
									let patch_source_idx = self.patch_source_idx;

									std::thread::spawn(move || {
										let report = |m: &str, p: u8| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); };
										// Stage 1: basic file layout
										report("Preparing files", 2);
										let _ = perform_basic_install(&plan, |msg, pct| { let scaled = 0 + ((pct as u16 * 25) / 100) as u8; let _ = tx.send(JobProgress { message: msg.to_string(), percent: scaled }); });

										// Async stages in a runtime
										let rt = tokio::runtime::Runtime::new().unwrap();
										rt.block_on(async move {
											use rtxlauncher_core::GitHubRateLimit;
											// Stage 2: install RTX Remix
											let remix_sources: [(&str, &str); 2] = [("sambow23", "dxvk-remix-gmod"), ("NVIDIAGameWorks", "rtx-remix")];
											let (owner_r, repo_r) = remix_sources[remix_source_idx.min(1)];
											let mut rl = GitHubRateLimit::default();
											let remix_list = fetch_releases(owner_r, repo_r, &mut rl).await.unwrap_or_default();
											if !remix_list.is_empty() {
												let rel = remix_list[remix_release_idx.min(remix_list.len()-1)].clone();
												let base = exec_dir.clone();
												let _ = install_remix_from_release(&rel, &base, |m,p| { let scaled = 25 + ((p as u16 * 35) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
												// Save installed remix version
												let label = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
												if let Ok(store) = rtxlauncher_core::SettingsStore::new() {
													if let Ok(mut s) = store.load() { s.installed_remix_version = Some(label); let _ = store.save(&s); }
												}
											}

											// Stage 3: install fixes package
											let fixes_sources: [(&str, &str); 2] = [("Xenthio", "gmod-rtx-fixes-2"), ("Xenthio", "RTXFixes")];
											let (owner_f, repo_f) = fixes_sources[fixes_source_idx.min(1)];
											let mut rl2 = GitHubRateLimit::default();
											let fixes_list = fetch_releases(owner_f, repo_f, &mut rl2).await.unwrap_or_default();
											if !fixes_list.is_empty() {
												let rel = fixes_list[fixes_release_idx.min(fixes_list.len()-1)].clone();
												let base = exec_dir.clone();
												let _ = install_fixes_from_release(&rel, &base, Some(DEFAULT_IGNORE_PATTERNS), |m,p| { let scaled = 60 + ((p as u16 * 25) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
												// Save installed fixes version
												let label = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
												if let Ok(store) = rtxlauncher_core::SettingsStore::new() { if let Ok(mut s) = store.load() { s.installed_fixes_version = Some(label); let _ = store.save(&s); } }
											}

											// Stage 4: apply binary patches
											let patch_sources: [(&str, &str); 3] = [("sambow23", "SourceRTXTweaks"), ("BlueAmulet", "SourceRTXTweaks"), ("Xenthio", "SourceRTXTweaks")];
											let (owner_p, repo_p) = patch_sources[patch_source_idx.min(2)];
											let base = exec_dir.clone();
											match apply_patches_from_repo(owner_p, repo_p, "applypatch.py", &base, |m,p| { let scaled = 85 + ((p as u16 * 15) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled.min(99) }); }).await {
												Ok(_) => {
													if let Ok(store) = rtxlauncher_core::SettingsStore::new() { if let Ok(mut s) = store.load() { s.installed_patches_commit = Some(format!("{}/{}", owner_p, repo_p)); let _ = store.save(&s); } }
												}
												Err(_) => {}
											}
											let _ = tx.send(JobProgress { message: "Quick install complete".into(), percent: 100 });
										});
									});
								}
							}
						}
					});
					ui.separator();
					self.show_logs(ui);
				}
				Tab::Mount => {
					ui.heading("Mounting");
					ui.add_enabled_ui(!self.is_running, |ui| {
						// Mountable games list
						ui.label("Detected mountable games:");
						let mut detected: Vec<(&'static str, Option<std::path::PathBuf>, &'static str)> = vec![
							("Half-Life 2 RTX", rtxlauncher_core::detect_install_folder_path("Half-Life 2 RTX"), "hl2rtx"),
							("Portal RTX", rtxlauncher_core::detect_install_folder_path("Portal RTX"), "portalrtx"),
						];
						for (name, path_opt, mod_folder) in detected.drain(..) {
							let label = if let Some(p) = path_opt { format!("{} — {}", name, p.display()) } else { format!("{} — not found", name) };
							if ui.button(label).clicked() {
								self.mount_game_folder = mod_folder.to_string();
								self.mount_remix_mod = mod_folder.to_string();
								self.add_toast(&format!("Selected {}", name), egui::Color32::LIGHT_GREEN);
							}
						}
						ui.separator();
						let mut gf = self.mount_game_folder.clone();
						ui.horizontal(|ui| { ui.label("Game folder (source content):"); ui.text_edit_singleline(&mut gf); });
						self.mount_game_folder = gf;
						let mut rm = self.mount_remix_mod.clone();
						ui.horizontal(|ui| { ui.label("Remix mod folder:"); ui.text_edit_singleline(&mut rm); });
						self.mount_remix_mod = rm;
						if ui.button("Mount").clicked() {
							let gf = self.mount_game_folder.clone();
							let rm = self.mount_remix_mod.clone();
							let mut tmp = String::new();
							let _ = mount_game(&gf, "Half-Life 2 RTX", &rm, |m| { tmp.push_str(m); tmp.push('\n'); });
							self.append_log(&tmp);
							self.add_toast("Mounted content", egui::Color32::GREEN);
						}
						if ui.button("Unmount").clicked() {
							let gf = self.mount_game_folder.clone();
							let rm = self.mount_remix_mod.clone();
							let mut tmp = String::new();
							let _ = unmount_game(&gf, "Half-Life 2 RTX", &rm, |m| { tmp.push_str(m); tmp.push('\n'); });
							self.append_log(&tmp);
							self.add_toast("Unmounted content", egui::Color32::GREEN);
						}
						ui.separator();
						// USDA fixes moved here
						if ui.button("Apply USDA fixes for hl2rtx").clicked() {
							let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
							self.current_job = Some(rx);
							self.is_running = true;
							std::thread::spawn(move || {
								let rt = tokio::runtime::Runtime::new().unwrap();
								rt.block_on(async move {
									let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
									let _ = apply_usda_fixes(&base, "hl2rtx", |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
								});
							});
						}
					});
					ui.separator();
					self.show_logs(ui);
				}
				Tab::Repositories => { self.repositories_tab(ui); }
				Tab::Settings => {
					ui.heading("Settings");
					let mut path_display = self.settings.manually_specified_install_path.clone().unwrap_or_default();
					ui.horizontal(|ui| {
						ui.label("Original Garry's Mod path:");
						ui.text_edit_singleline(&mut path_display);
						if ui.add_enabled(!self.is_running, egui::Button::new("Browse")).clicked() {
							if let Some(p) = FileDialog::new().set_directory("C:/").pick_folder() {
								self.settings.manually_specified_install_path = Some(p.display().to_string());
								let _ = self.settings_store.save(&self.settings);
							}
						}
						if ui.add_enabled(!self.is_running, egui::Button::new("Auto-detect (Steam)")).clicked() {
							if let Some(p) = detect_gmod_install_folder() {
								self.settings.manually_specified_install_path = Some(p.display().to_string());
								let _ = self.settings_store.save(&self.settings);
							}
						}
					});
					ui.horizontal(|ui| {
						ui.label("GitHub PAT (optional):");
						let mut pat = load_personal_access_token().unwrap_or_default();
						ui.add(egui::TextEdit::singleline(&mut pat).password(true).desired_width(200.0));
						if ui.button("Save PAT").clicked() {
							let _ = set_personal_access_token(if pat.trim().is_empty() { None } else { Some(pat.clone()) });
							self.add_toast("Saved PAT", egui::Color32::LIGHT_GREEN);
						}
					});
					ui.separator();
					ui.label("Launch options");
					// Resolution dropdown (Auto, current window size, common presets)
					let mut resolutions: Vec<(u32, u32)> = vec![(1280,720),(1280,800),(1366,768),(1440,900),(1600,900),(1680,1050),(1920,1080),(1920,1200),(2560,1080),(2560,1440),(2560,1600),(3440,1440),(3840,2160)];
					let win_size = ctx.input(|i| i.screen_rect.size());
					let current_px = (win_size.x.round() as u32, win_size.y.round() as u32);
					if current_px.0 > 0 && current_px.1 > 0 && !resolutions.contains(&current_px) { resolutions.insert(0, current_px); }
					resolutions.dedup();
					ui.horizontal(|ui| {
						ui.label("Resolution:");
						let sel_w = self.settings.width.unwrap_or(0);
						let sel_h = self.settings.height.unwrap_or(0);
						let is_custom = !(sel_w > 0 && sel_h > 0 && resolutions.contains(&(sel_w, sel_h)));
						let selected_text = if is_custom { "Custom".to_string() } else { format!("{}x{}", sel_w, sel_h) };
						egui::ComboBox::from_id_source("res-dropdown").selected_text(selected_text).show_ui(ui, |ui| {
							// Custom at top
							if ui.selectable_label(is_custom, "Custom").clicked() {
								// Mark as custom by clearing preset selection
								self.settings.width = None;
								self.settings.height = None;
								let _ = self.settings_store.save(&self.settings);
							}
							for (w,h) in resolutions.iter().cloned() {
								let label = format!("{}x{}", w,h);
								let is_sel = sel_w==w && sel_h==h;
								if ui.selectable_label(is_sel, label).clicked() {
									self.settings.width = Some(w);
									self.settings.height = Some(h);
									let _ = self.settings_store.save(&self.settings);
								}
							}
						});
					});
					let sel_w2 = self.settings.width.unwrap_or(0);
					let sel_h2 = self.settings.height.unwrap_or(0);
					let is_custom2 = !(sel_w2 > 0 && sel_h2 > 0 && resolutions.contains(&(sel_w2, sel_h2)));
					if is_custom2 {
						ui.horizontal(|ui| {
							let mut w = self.settings.width.unwrap_or_default();
							ui.label("Width");
							if ui.add(egui::DragValue::new(&mut w).clamp_range(0..=16384)).changed() {
								self.settings.width = Some(w);
								let _ = self.settings_store.save(&self.settings);
							}
							let mut h = self.settings.height.unwrap_or_default();
							ui.label("Height");
							if ui.add(egui::DragValue::new(&mut h).clamp_range(0..=16384)).changed() {
								self.settings.height = Some(h);
								let _ = self.settings_store.save(&self.settings);
							}
						});
					}
					if ui.checkbox(&mut self.settings.console_enabled, "Enable console").changed() { let _ = self.settings_store.save(&self.settings); }
					if ui.checkbox(&mut self.settings.load_workshop_addons, "Load Workshop Addons").changed() { let _ = self.settings_store.save(&self.settings); }
					if ui.checkbox(&mut self.settings.disable_chromium, "Disable Chromium").changed() { let _ = self.settings_store.save(&self.settings); }
					if ui.checkbox(&mut self.settings.developer_mode, "Developer mode").changed() { let _ = self.settings_store.save(&self.settings); }
					if ui.checkbox(&mut self.settings.tools_mode, "Particle Editor Mode").changed() { let _ = self.settings_store.save(&self.settings); }
					ui.horizontal(|ui| {
						ui.label("Custom args:");
						let mut custom = self.settings.custom_launch_options.clone().unwrap_or_default();
						if ui.text_edit_singleline(&mut custom).changed() { self.settings.custom_launch_options = if custom.trim().is_empty() { None } else { Some(custom) }; let _ = self.settings_store.save(&self.settings); }
					});

					#[cfg(unix)]
					{
						ui.separator();
						ui.label("Linux launch (Proton)");
						if ui.checkbox(&mut self.settings.linux_enable_proton_log, "Enable PROTON_LOG").changed() { let _ = self.settings_store.save(&self.settings); }
						ui.horizontal(|ui| {
							ui.label("Proton build:");
							let builds = list_proton_builds(&self.settings);
							let labels: Vec<String> = builds.iter().map(|(label, _)| label.clone()).collect();
							let selected_text = self.settings.linux_selected_proton_label.clone().unwrap_or_else(|| labels.first().cloned().unwrap_or_else(|| "(none found)".to_string()));
							egui::ComboBox::from_id_source("linux-proton-build").selected_text(selected_text).show_ui(ui, |ui| {
								for (label, path) in builds.iter() {
									let is_sel = self.settings.linux_selected_proton_label.as_ref().map(|s| s == label).unwrap_or(false);
									if ui.selectable_label(is_sel, label).clicked() {
										self.settings.linux_selected_proton_label = Some(label.clone());
										self.settings.linux_proton_path = Some(path.clone());
										let _ = self.settings_store.save(&self.settings);
									}
								}
							});
						});
						ui.horizontal(|ui| {
							ui.label("Proton path override:");
							let mut v = self.settings.linux_proton_path.clone().unwrap_or_default();
							if ui.text_edit_singleline(&mut v).changed() { self.settings.linux_proton_path = if v.trim().is_empty() { None } else { Some(v) }; let _ = self.settings_store.save(&self.settings); }
						});
						ui.horizontal(|ui| {
							ui.label("Steam root override:");
							let mut v = self.settings.linux_steam_root_override.clone().unwrap_or_default();
							if ui.text_edit_singleline(&mut v).changed() { self.settings.linux_steam_root_override = if v.trim().is_empty() { None } else { Some(v) }; let _ = self.settings_store.save(&self.settings); }
						});
					}
					#[cfg(windows)]
					{
						if !is_elevated() {
							if ui.button("Relaunch as Administrator").clicked() {
								let exe = std::env::current_exe().ok();
								if let Some(exe) = exe {
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
					#[cfg(not(windows))]
					{
						if !is_elevated() {
							ui.label("Administrative relaunch is available on Windows only.");
						}
					}
				}
				Tab::About => {
					ui.heading("About");
					ui.separator();
					ui.label("A recreation of Xenthio's original .NET launcher, aimed for cross-platform support like Linux, in addition to upcoming features.");
					ui.separator();
					// Launcher version (git hash)
					let git = option_env!("GIT_COMMIT_HASH").unwrap_or("unknown");
					ui.label(format!("Launcher version: {}", git));
					// GMod game version: best-effort based on vanilla path mtime
					if let Some(p) = rtxlauncher_core::detect_gmod_install_folder() {
						if let Ok(meta) = std::fs::metadata(&p) {
							if let Ok(modified) = meta.modified() {
								use chrono::{DateTime, Local};
								let dt: DateTime<Local> = modified.into();
								ui.label(format!("GMod install modified: {}", dt.format("%d/%m/%Y %H:%M")));
							}
						}
					}
					let remix_v = self.settings.installed_remix_version.clone().unwrap_or_else(|| "(unknown)".into());
					let fixes_v = self.settings.installed_fixes_version.clone().unwrap_or_else(|| "(unknown)".into());
					let patch_c = self.settings.installed_patches_commit.clone().unwrap_or_else(|| "(none)".into());
					ui.label(format!("Installed Remix: {}", remix_v));
					ui.label(format!("Installed Fixes: {}", fixes_v));
					ui.label(format!("Applied Patches: {}", patch_c));
				}
				
			}
		});
		// Render any modals that may be active (outside of panel)
		self.render_update_dialog(ctx);
		self.render_reapply_dialog(ctx);
		self.draw_toasts(ctx);
	}
}

impl LauncherApp {
	fn append_log(&mut self, msg: &str) { if !self.log.is_empty() { self.log.push('\n'); } self.log.push_str(msg); }
	fn add_toast(&mut self, msg: &str, color: egui::Color32) { self.toasts.push(Toast { msg: msg.to_string(), color, until: std::time::Instant::now() + std::time::Duration::from_secs(4) }); }
	fn poll_job(&mut self) {
		if self.current_job.is_none() { return; }
		let mut finished = false;
		// take ownership to avoid borrow conflicts
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				self.progress = p.percent;
				self.append_log(&p.message);
				if p.percent >= 100 { self.is_running = false; finished = true; let color = if p.message.to_lowercase().contains("error") { egui::Color32::RED } else { egui::Color32::LIGHT_GREEN }; self.add_toast("Job completed", color); }
			}
			// if not finished, put receiver back
			if !finished { self.current_job = Some(rx); }
		}
		// Refresh settings from disk on completion so version fields update
		if !self.is_running {
			if let Ok(s) = self.settings_store.load() { self.settings = s; }
		}
	}
	fn draw_toasts(&mut self, ctx: &egui::Context) {
		let now = std::time::Instant::now();
		self.toasts.retain(|t| t.until > now);
		let mut y = 12.0;
		for (i, t) in self.toasts.iter().enumerate() {
			egui::Area::new(egui::Id::new(format!("toast-{i}"))).fixed_pos(egui::pos2(220.0, y)).show(ctx, |ui| { ui.colored_label(t.color, &t.msg); });
			y += 22.0;
		}
	}

	fn show_logs(&mut self, ui: &mut egui::Ui) {
		ui.label("Logs:");
		let avail = ui.available_size();
		let height = avail.y.max(200.0); // fill remaining vertical space with a sensible minimum
		egui::ScrollArea::vertical()
			.stick_to_bottom(true)
			.auto_shrink([false, false])
			.max_height(height)
			.show(ui, |ui| {
				ui.set_min_height(height);
				ui.monospace(&self.log);
			});
	}

	fn repositories_tab(&mut self, ui: &mut egui::Ui) {
		ui.heading("Repositories");
		ui.separator();

		// Update Base Game section
		ui.group(|ui| {
			ui.heading("Base Game Updates");
			if ui.add_enabled(!self.is_running, egui::Button::new("Update Base Game")).clicked() {
				self.prepare_update_dialog();
				self.show_update_dialog = true;
			}
		});

		if !self.remix_loading && self.remix_releases.is_empty() { self.start_fetch_releases(true); }
		if !self.fixes_loading && self.fixes_releases.is_empty() { self.start_fetch_releases(false); }

		// Remix section
		ui.group(|ui| {
			ui.heading("NVIDIA RTX Remix");
			let remix_sources: [(&str, &str, &str); 2] = [
				("sambow23/dxvk-remix-gmod", "sambow23", "dxvk-remix-gmod"),
				("(OFFICIAL) NVIDIAGameWorks/rtx-remix", "NVIDIAGameWorks", "rtx-remix"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_source("remix-source").selected_text(remix_sources[self.remix_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in remix_sources.iter().enumerate() {
						if ui.selectable_label(self.remix_source_idx == i, *label).clicked() { self.remix_source_idx = i; self.start_fetch_releases(true); }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Version");
				let label = |r: &GitHubRelease| r.name.clone().unwrap_or_else(|| r.tag_name.clone().unwrap_or_default());
				let selected_text = if self.remix_releases.is_empty() { if self.remix_loading { "Loading...".to_string() } else { "No releases".to_string() } } else { label(&self.remix_releases[self.remix_release_idx.min(self.remix_releases.len()-1)]) };
				egui::ComboBox::from_id_source("remix-version").selected_text(selected_text).show_ui(ui, |ui| {
					for (i, r) in self.remix_releases.iter().enumerate() {
						let text = label(r);
						if ui.selectable_label(self.remix_release_idx == i, text).clicked() { self.remix_release_idx = i; }
					}
				});
				if ui.add_enabled(!self.is_running && !self.remix_releases.is_empty(), egui::Button::new("Install/Update")).clicked() {
					let rel = self.remix_releases[self.remix_release_idx].clone();
					let rel_label = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					self.current_job = Some(rx);
					self.is_running = true;
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
							let _ = install_remix_from_release(&rel, &base, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
							if let Ok(store) = rtxlauncher_core::SettingsStore::new() {
								if let Ok(mut s) = store.load() {
									s.installed_remix_version = Some(rel_label);
									let _ = store.save(&s);
								}
							}
						});
					});
				}
			});
		});

		ui.add_space(8.0);

		// Fixes section
		ui.group(|ui| {
			ui.heading("Fixes Package");
			let fixes_sources: [(&str, &str, &str); 2] = [
				("Xenthio/gmod-rtx-fixes-2 (Any)", "Xenthio", "gmod-rtx-fixes-2"),
				("Xenthio/RTXFixes (gmod_main)", "Xenthio", "RTXFixes"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_source("fixes-source").selected_text(fixes_sources[self.fixes_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in fixes_sources.iter().enumerate() {
						if ui.selectable_label(self.fixes_source_idx == i, *label).clicked() { self.fixes_source_idx = i; self.start_fetch_releases(false); }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Version");
				let label = |r: &GitHubRelease| r.name.clone().unwrap_or_else(|| r.tag_name.clone().unwrap_or_default());
				let selected_text = if self.fixes_releases.is_empty() { if self.fixes_loading { "Loading...".to_string() } else { "No packages".to_string() } } else { label(&self.fixes_releases[self.fixes_release_idx.min(self.fixes_releases.len()-1)]) };
				egui::ComboBox::from_id_source("fixes-version").selected_text(selected_text).show_ui(ui, |ui| {
					for (i, r) in self.fixes_releases.iter().enumerate() {
						let text = label(r);
						if ui.selectable_label(self.fixes_release_idx == i, text).clicked() { self.fixes_release_idx = i; }
					}
				});
				if ui.add_enabled(!self.is_running && !self.fixes_releases.is_empty(), egui::Button::new("Install/Update")).clicked() {
					let rel = self.fixes_releases[self.fixes_release_idx].clone();
					let rel_label = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					self.current_job = Some(rx);
					self.is_running = true;
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
							let _ = install_fixes_from_release(&rel, &base, Some(DEFAULT_IGNORE_PATTERNS), |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
							if let Ok(store) = rtxlauncher_core::SettingsStore::new() {
								if let Ok(mut s) = store.load() {
									s.installed_fixes_version = Some(rel_label);
									let _ = store.save(&s);
								}
							}
						});
					});
				}
			});
		});

		ui.add_space(8.0);

		// Binary patches (not supported in Rust rewrite)
		ui.group(|ui| {
			ui.heading("Binary Patches");
			let patch_sources: [(&str, &str, &str); 3] = [
				("sambow23/SourceRTXTweaks", "sambow23", "SourceRTXTweaks"),
				("BlueAmulet/SourceRTXTweaks", "BlueAmulet", "SourceRTXTweaks"),
				("Xenthio/SourceRTXTweaks", "Xenthio", "SourceRTXTweaks"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_source("patch-source").selected_text(patch_sources[self.patch_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in patch_sources.iter().enumerate() {
						if ui.selectable_label(self.patch_source_idx == i, *label).clicked() { self.patch_source_idx = i; }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Action");
				if ui.add_enabled(!self.is_running, egui::Button::new("Apply Patches")).clicked() {
					let (owner, repo) = { let s = patch_sources[self.patch_source_idx]; (s.1.to_string(), s.2.to_string()) };
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					self.current_job = Some(rx);
					self.is_running = true;
					let install_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let owner = owner;
							let repo = repo;
							match apply_patches_from_repo(&owner, &repo, "applypatch.py", &install_dir, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await {
								Ok(res) => {
									let _ = tx.send(JobProgress { message: format!("Patched {} file(s).", res.files_patched), percent: 100 });
									for w in res.warnings { let _ = tx.send(JobProgress { message: format!("Warning: {}", w), percent: 100 }); }
									if let Ok(store) = rtxlauncher_core::SettingsStore::new() {
										if let Ok(mut s) = store.load() {
											s.installed_patches_commit = Some(format!("{}/{}", owner, repo));
											let _ = store.save(&s);
										}
									}
								}
								Err(e) => {
									let _ = tx.send(JobProgress { message: format!("Error: {}", e), percent: 100 });
								}
							}
						});
					});
				}
			});
		});

		ui.separator();
		self.show_logs(ui);

		// Poll async release fetchers
		if let Some(rx) = self.remix_rx.take() { if let Ok(list) = rx.try_recv() { self.remix_releases = list; self.remix_release_idx = 0; self.remix_loading = false; } else { self.remix_rx = Some(rx); } }
		if let Some(rx) = self.fixes_rx.take() { if let Ok(list) = rx.try_recv() { self.fixes_releases = list; self.fixes_release_idx = 0; self.fixes_loading = false; } else { self.fixes_rx = Some(rx); } }

		// Modals rendered from update() with ctx
	}

	fn start_fetch_releases(&mut self, remix: bool) {
		let (owner, repo) = if remix {
			match self.remix_source_idx { 0 => ("sambow23", "dxvk-remix-gmod"), _ => ("NVIDIAGameWorks", "rtx-remix") }
		} else {
			match self.fixes_source_idx { 0 => ("Xenthio", "gmod-rtx-fixes-2"), _ => ("Xenthio", "RTXFixes") }
		};
		let (tx, rx) = std::sync::mpsc::channel::<Vec<GitHubRelease>>();
		if remix { self.remix_rx = Some(rx); self.remix_loading = true; } else { self.fixes_rx = Some(rx); self.fixes_loading = true; }
		std::thread::spawn(move || {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async move {
				let mut rl = GitHubRateLimit::default();
				let list = fetch_releases(owner, repo, &mut rl).await.unwrap_or_default();
				let _ = tx.send(list);
			});
		});
	}
}

impl LauncherApp {
    fn prepare_update_dialog(&mut self) {
        self.update_folder_options.clear();
        self.update_folder_selected.clear();
        let vanilla = self.settings.manually_specified_install_path.clone()
            .or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
        if let Some(v) = vanilla {
            let root = std::path::PathBuf::from(v);
            // root-level only (top-level folders)
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
    }

    fn render_update_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_update_dialog { return; }
        egui::Window::new("Update Base Game")
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Select folders to copy from the vanilla installation:");
                let mut any = false;
                for (i, label) in self.update_folder_options.iter().enumerate() {
                    let mut sel = self.update_folder_selected[i];
                    if ui.checkbox(&mut sel, label).changed() { self.update_folder_selected[i] = sel; }
                    any |= sel;
                }
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
        let selected_prefixes: Vec<String> = self.update_folder_options.iter().cloned()
            .zip(self.update_folder_selected.iter().cloned())
            .filter_map(|(l, s)| if s { Some(l) } else { None }).collect();
        let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
        self.current_job = Some(rx);
        self.is_running = true;
        std::thread::spawn(move || {
            // Locate paths
            let src = rtxlauncher_core::detect_gmod_install_folder().unwrap_or_default();
            let dst = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
            // Detect all updates then filter
            let updates = rtxlauncher_core::detect_updates(&src, &dst).unwrap_or_default();
            let include_root_execs = selected_prefixes.iter().any(|p| p == "bin");
            let filtered: Vec<_> = updates.into_iter().filter(|u| {
                if selected_prefixes.is_empty() { return false; }
                let rp = &u.relative_path;
                // root-level files (e.g., gmod.exe, hl2.exe, steam_appid.txt)
                if !rp.contains('/') {
                    return include_root_execs && (rp.eq_ignore_ascii_case("gmod.exe") || rp.eq_ignore_ascii_case("hl2.exe") || rp.eq_ignore_ascii_case("steam_appid.txt"));
                }
                // folder selection: match top-level prefix + '/'
                for p in &selected_prefixes {
                    let prefix = format!("{}/", p);
                    if rp.starts_with(&prefix) || rp == p { return true; }
                }
                false
            }).collect();
            let _ = rtxlauncher_core::apply_updates(&filtered, |m,p| {
                let scaled = ((p as u16 * 90) / 100) as u8; // scale into 0-90
                let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled });
            });
            let _ = tx.send(JobProgress { message: "Base game update complete".into(), percent: 100 });
        });
        // After job completes, show reapply dialog next frame
        self.show_reapply_dialog = true;
        self.reapply_fixes = true;
        self.reapply_patches = true;
    }

    fn render_reapply_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_reapply_dialog || self.is_running { return; }
        egui::Window::new("Reapply Components?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Reapply components after updating base game?");
                ui.checkbox(&mut self.reapply_fixes, "Reapply Fixes Package");
                ui.checkbox(&mut self.reapply_patches, "Reapply Binary Patches");
                ui.horizontal(|ui| {
                    if ui.button("Proceed").clicked() {
                        self.show_reapply_dialog = false;
                        self.trigger_reapply_jobs();
                    }
                    if ui.button("Skip").clicked() { self.show_reapply_dialog = false; }
                });
            });
    }

    fn trigger_reapply_jobs(&mut self) {
        // Reapply fixes and/or patches using current UI selections
        if self.reapply_fixes {
            if let Some(rel) = self.fixes_releases.get(self.fixes_release_idx).cloned() {
                let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
                self.current_job = Some(rx);
                self.is_running = true;
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
                        let _ = install_fixes_from_release(&rel, &base, Some(DEFAULT_IGNORE_PATTERNS), |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
                    });
                });
            }
        }
        if self.reapply_patches {
            let (owner, repo) = {
                let s = [("sambow23","SourceRTXTweaks"),("BlueAmulet","SourceRTXTweaks"),("Xenthio","SourceRTXTweaks")][self.patch_source_idx.min(2)];
                (s.0.to_string(), s.1.to_string())
            };
            let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
            self.current_job = Some(rx);
            self.is_running = true;
            let install_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let _ = apply_patches_from_repo(&owner, &repo, "applypatch.py", &install_dir, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
                });
            });
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	init_logging();
	let _store = SettingsStore::new()?;
	let native_options = eframe::NativeOptions::default();
	eframe::run_native(
		"RTXLauncher (Rust)",
		native_options,
		Box::new(|_cc| Ok(Box::new(LauncherApp::default()))),
	).unwrap();
	Ok(())
}


