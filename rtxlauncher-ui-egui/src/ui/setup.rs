use eframe::egui;
use rtxlauncher_core::{JobProgress, InstallPlan, detect_gmod_install_folder, perform_basic_install, GitHubRateLimit, fetch_releases, install_remix_from_release, install_fixes_from_release, apply_patches_from_repo};

pub struct SetupState {
	pub is_running: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
	pub progress: u8,
	pub setup_completed: bool,
	pub show_quick_install_dialog: bool,
}

impl Default for SetupState {
	fn default() -> Self {
		Self {
			is_running: false,
			current_job: None,
			progress: 0,
			setup_completed: false,
			show_quick_install_dialog: false,
		}
	}
}

impl SetupState {
	pub fn poll_job(&mut self, global_log: &mut String) -> bool {
		if self.current_job.is_none() { return false; }
		let mut finished = false;
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				self.progress = p.percent;
				// Append to global log (deduplicated)
				crate::app::append_line_dedup(global_log, &p.message);
				if p.percent >= 100 { 
					self.is_running = false; 
					self.setup_completed = true;
					finished = true; 
				}
			}
			if !finished { self.current_job = Some(rx); }
		}
		finished
	}
}

pub fn render_setup_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	let job_finished = {
		let st = &mut app.setup;
		st.poll_job(&mut app.log)
	};
	if job_finished {
		// Reload settings when a job finishes to update version info
		if let Ok(new_settings) = app.settings_store.load() {
			app.settings = new_settings;
		}
		// Mark setup as completed in settings
		app.settings.setup_completed = Some(true);
		let _ = app.settings_store.save(&app.settings);
		app.add_toast("Setup completed successfully!", egui::Color32::LIGHT_GREEN);
	}

	// Use a simpler approach: center vertically using available space
	ui.allocate_ui_with_layout(
		ui.available_size(),
		egui::Layout::top_down(egui::Align::Center),
		|ui| {
			// Add flexible space at the top to push content to center
			let available_height = ui.available_height();
			ui.add_space(available_height * 0.25); // Start content at 25% down
			
			ui.vertical_centered(|ui| {
				// RTX Launcher logo/title
				ui.heading(egui::RichText::new("Welcome to the Garry's Mod RTX Launcher").size(28.0));
				ui.add_space(20.0);

				// Check if this is a returning user with completed setup
				let is_returning_user = matches!(app.settings.setup_completed, Some(true)) && !app.setup.is_running && !app.setup.setup_completed;

				// Setup status or progress
				if app.setup.is_running {
					ui.label(egui::RichText::new("Setting up Garry's Mod RTX...").size(18.0));
					ui.add_space(10.0);
					
					let pct = app.setup.progress as f32 / 100.0;
					let bar = egui::ProgressBar::new(pct)
						.text(format!("{}%", app.setup.progress))
						.desired_width(400.0)
						.desired_height(20.0);
					ui.add(bar);
					ui.add_space(10.0);
					ui.label("This may take several minutes depending on your internet connection...");
				} else if is_returning_user {
					// Returning user with completed setup
					ui.colored_label(egui::Color32::LIGHT_GREEN, 
						egui::RichText::new("Garry's Mod RTX is Already Installed").size(20.0));
					ui.add_space(10.0);
					ui.label("Your Garry's Mod RTX installation is ready to use!");
					ui.add_space(15.0);
					ui.label("You can:");
					ui.add_space(5.0);
					ui.horizontal(|ui| {
						ui.add_space(20.0);
						ui.vertical(|ui| {
							ui.label("• Launch the game using the Launch Game button");
							ui.label("• Adjust settings in the Settings tab");
							ui.label("• Mount content from other games in the Mounting tab");
						});
					});
					ui.add_space(20.0);
					
					// Offer reinstall option
					ui.separator();
					ui.add_space(15.0);
					ui.label(egui::RichText::new("Need to reinstall?").size(16.0));
					ui.add_space(10.0);
					if ui.add_sized([200.0, 35.0], 
						egui::Button::new(egui::RichText::new("Reinstall Garry's Mod RTX").size(14.0))
							.rounding(egui::Rounding::same(6.0))
					).clicked() {
						start_quick_install(app);
					}
				} else if app.setup.setup_completed {
					ui.colored_label(egui::Color32::LIGHT_GREEN, 
						egui::RichText::new("Setup Complete!").size(20.0));
					ui.add_space(10.0);
					ui.label("Garry's Mod RTX has been successfully installed and configured.");
					ui.add_space(15.0);
					ui.label("You can now:");
					ui.add_space(5.0);
					ui.horizontal(|ui| {
						ui.add_space(20.0);
						ui.vertical(|ui| {
							ui.label("• Launch the game using the Launch Game button");
							ui.label("• Adjust screen resolution and other options in Settings");
							ui.label("• Mount content from other games in the Mounting tab");
						});
					});
				} else {
					// First-time setup prompt
					ui.label(egui::RichText::new("Would you like to run the quick install process?").size(18.0));
					ui.add_space(15.0);
					
					ui.label("This will automatically:");
					ui.add_space(5.0);
					
					ui.horizontal(|ui| {
						ui.add_space(20.0);
						ui.vertical(|ui| {
							ui.label("• Download and install RTX Remix");
							ui.label("• Install community fixes and patches");
							ui.label("• Configure optimal settings");
							ui.label("• Copy necessary files from your Steam installation");
						});
					});
					
					ui.add_space(25.0);
					
					// Check if Garry's Mod installation is detected
					let gmod_detected = detect_gmod_install_folder().is_some();
					if !gmod_detected {
						ui.colored_label(egui::Color32::YELLOW, 
							"⚠ Garry's Mod installation not automatically detected");
						ui.label("You may need to specify the installation path in Settings.");
						ui.add_space(10.0);
					}
					
					ui.horizontal(|ui| {
						// Center the buttons
						let button_width = 140.0;
						let button_height = 45.0;
						let spacing = 20.0;
						let total_width = button_width * 2.0 + spacing;
						let available_width = ui.available_width();
						let offset = (available_width - total_width) / 2.0;
						ui.add_space(offset);
						
						if ui.add_sized([button_width, button_height], 
							egui::Button::new(egui::RichText::new("Quick Install").size(16.0))
								.rounding(egui::Rounding::same(8.0))
						).clicked() {
							start_quick_install(app);
						}
						
						ui.add_space(spacing);
						
						if ui.add_sized([button_width, button_height], 
							egui::Button::new(egui::RichText::new("Skip for Now").size(16.0))
								.rounding(egui::Rounding::same(8.0))
						).clicked() {
							// Mark setup as completed but without installation
							app.settings.setup_completed = Some(false);
							let _ = app.settings_store.save(&app.settings);
							app.selected = crate::app::Tab::Settings;
							app.add_toast("You can run installation later from the Repositories tab", egui::Color32::LIGHT_BLUE);
						}
					});
				}
			});
		},
	);
}

fn start_quick_install(app: &mut crate::app::LauncherApp) {
	let vanilla_opt = app.settings.manually_specified_install_path.clone()
		.or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
	
	if let Some(vanilla) = vanilla_opt {
		if let Ok(exec_dir) = std::env::current_exe().map(|p| p.parent().unwrap().to_path_buf()) {
			let plan = InstallPlan { 
				vanilla: std::path::PathBuf::from(vanilla), 
				rtx: exec_dir.clone() 
			};
			
			let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
			app.setup.current_job = Some(rx);
			app.setup.is_running = true;
			
			// Use default source indices (first option for each)
			let remix_source_idx = 0;
			let remix_release_idx = 0;
			let fixes_source_idx = 0;
			let fixes_release_idx = 0;
			let patch_source_idx = 0;
			
			let settings_store = app.settings_store.clone();
			let mut settings = app.settings.clone();
			
			std::thread::spawn(move || {
				let tx_clone = tx.clone();
				let report = |m: &str, p: u8| { 
					let _ = tx_clone.send(JobProgress { 
						message: m.to_string(), 
						percent: p 
					}); 
				};
				
				report("Preparing installation...", 2);
				let tx_clone2 = tx.clone();
				let _ = perform_basic_install(&plan, |msg, pct| { 
					let scaled = 0 + ((pct as u16 * 25) / 100) as u8; 
					let _ = tx_clone2.send(JobProgress { 
						message: msg.to_string(), 
						percent: scaled 
					}); 
				});
				
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async move {
					// Install RTX Remix
					report("Downloading RTX Remix...", 25);
					let remix_sources: [(&str, &str); 2] = [("sambow23", "dxvk-remix-gmod"), ("NVIDIAGameWorks", "rtx-remix")];
					let (owner_r, repo_r) = remix_sources[remix_source_idx.min(1)];
					let mut rl = GitHubRateLimit::default();
					let remix_list = fetch_releases(owner_r, repo_r, &mut rl).await.unwrap_or_default();
					if !remix_list.is_empty() {
						let rel = remix_list[remix_release_idx.min(remix_list.len()-1)].clone();
						let base = exec_dir.clone();
						let result = install_remix_from_release(&rel, &base, |m,p| { 
							let scaled = 25 + ((p as u16 * 35) / 100) as u8; 
							let _ = tx.send(JobProgress { 
								message: m.to_string(), 
								percent: scaled 
							}); 
						}).await;
						if result.is_ok() {
							let rel_name = rel.name.unwrap_or_else(|| rel.tag_name.unwrap_or_default());
							settings.installed_remix_version = Some(rel_name);
						}
					}
					
					// Install fixes
					report("Installing community fixes...", 60);
					let fixes_sources: [(&str, &str); 2] = [("Xenthio", "gmod-rtx-fixes-2"), ("Xenthio", "RTXFixes")];
					let (owner_f, repo_f) = fixes_sources[fixes_source_idx.min(1)];
					let mut rl2 = GitHubRateLimit::default();
					let fixes_list = fetch_releases(owner_f, repo_f, &mut rl2).await.unwrap_or_default();
					if !fixes_list.is_empty() {
						let rel = fixes_list[fixes_release_idx.min(fixes_list.len()-1)].clone();
						let base = exec_dir.clone();
						let result = install_fixes_from_release(&rel, &base, Some(crate::app::DEFAULT_IGNORE_PATTERNS), |m,p| { 
							let scaled = 60 + ((p as u16 * 25) / 100) as u8; 
							let _ = tx.send(JobProgress { 
								message: m.to_string(), 
								percent: scaled 
							}); 
						}).await;
						if result.is_ok() {
							let rel_name = rel.name.unwrap_or_else(|| rel.tag_name.unwrap_or_default());
							settings.installed_fixes_version = Some(rel_name);
						}
					}
					
					// Apply patches
					report("Applying binary patches...", 85);
					let patch_sources: [(&str, &str); 3] = [("sambow23", "SourceRTXTweaks"), ("BlueAmulet", "SourceRTXTweaks"), ("Xenthio", "SourceRTXTweaks")];
					let (owner_p, repo_p) = patch_sources[patch_source_idx.min(2)];
					let base = exec_dir.clone();
					let result = apply_patches_from_repo(owner_p, repo_p, "applypatch.py", &base, |m,p| { 
						let scaled = 85 + ((p as u16 * 15) / 100) as u8; 
						let _ = tx.send(JobProgress { 
							message: m.to_string(), 
							percent: scaled.min(99) 
						}); 
					}).await;
					if result.is_ok() {
						let patch_info = format!("{}/{}", owner_p, repo_p);
						settings.installed_patches_commit = Some(patch_info);
					}
					
					// Save settings with all version information
					let _ = settings_store.save(&settings);
					let _ = tx.send(JobProgress { 
						message: "Setup complete! RTX Remix is ready to use.".into(), 
						percent: 100 
					});
				});
			});
		}
	} else {
		app.show_error_modal = Some("Could not detect Garry's Mod installation. Please specify the installation path in Settings first.".to_string());
	}
}
