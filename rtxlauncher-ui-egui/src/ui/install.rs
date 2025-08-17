use eframe::egui;
use rtxlauncher_core::{JobProgress, InstallPlan, detect_gmod_install_folder, perform_basic_install, GitHubRateLimit, fetch_releases, install_remix_from_release, install_fixes_from_release, GitHubRelease, apply_patches_from_repo};

pub struct InstallState {
	pub is_running: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
	pub progress: u8,
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
}

impl Default for InstallState {
	fn default() -> Self {
		Self {
			is_running: false,
			current_job: None,
			progress: 0,
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
		}
	}
}

impl InstallState {
	pub fn poll_job(&mut self, global_log: &mut String) -> bool {
		if self.current_job.is_none() { return false; }
		let mut finished = false;
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				self.progress = p.percent;
				// Append to global log
				if !global_log.is_empty() { global_log.push('\n'); }
				global_log.push_str(&p.message);
				if p.percent >= 100 { self.is_running = false; finished = true; }
			}
			if !finished { self.current_job = Some(rx); }
		}
		finished
	}
}

pub fn render_install_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	let job_finished = {
		let st = &mut app.install;
		st.poll_job(&mut app.log)
	};
	if job_finished {
		// Reload settings when a job finishes to update version info
		if let Ok(new_settings) = app.settings_store.load() {
			app.settings = new_settings;
		}
	}
	ui.heading("Install");
	ui.add_enabled_ui(!app.install.is_running, |ui| {
		if ui.button("Quick Install").clicked() {
			let vanilla_opt = app.settings.manually_specified_install_path.clone().or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
			if let Some(vanilla) = vanilla_opt {
				if let Ok(exec_dir) = std::env::current_exe().map(|p| p.parent().unwrap().to_path_buf()) {
					let plan = InstallPlan { vanilla: std::path::PathBuf::from(vanilla), rtx: exec_dir.clone() };
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					app.install.current_job = Some(rx);
					app.install.is_running = true;
					let remix_source_idx = app.install.remix_source_idx;
					let remix_release_idx = app.install.remix_release_idx;
					let fixes_source_idx = app.install.fixes_source_idx;
					let fixes_release_idx = app.install.fixes_release_idx;
					let patch_source_idx = app.install.patch_source_idx;
					let settings_store = app.settings_store.clone();
					let mut settings = app.settings.clone();
					std::thread::spawn(move || {
						let report = |m: &str, p: u8| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); };
						report("Preparing files", 2);
						let _ = perform_basic_install(&plan, |msg, pct| { let scaled = 0 + ((pct as u16 * 25) / 100) as u8; let _ = tx.send(JobProgress { message: msg.to_string(), percent: scaled }); });
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let remix_sources: [(&str, &str); 2] = [("sambow23", "dxvk-remix-gmod"), ("NVIDIAGameWorks", "rtx-remix")];
							let (owner_r, repo_r) = remix_sources[remix_source_idx.min(1)];
							let mut rl = GitHubRateLimit::default();
							let remix_list = fetch_releases(owner_r, repo_r, &mut rl).await.unwrap_or_default();
							if !remix_list.is_empty() {
								let rel = remix_list[remix_release_idx.min(remix_list.len()-1)].clone();
								let base = exec_dir.clone();
								let result = install_remix_from_release(&rel, &base, |m,p| { let scaled = 25 + ((p as u16 * 35) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
								if result.is_ok() {
									let rel_name = rel.name.unwrap_or_else(|| rel.tag_name.unwrap_or_default());
									settings.installed_remix_version = Some(rel_name);
								}
							}
							let fixes_sources: [(&str, &str); 2] = [("Xenthio", "gmod-rtx-fixes-2"), ("Xenthio", "RTXFixes")];
							let (owner_f, repo_f) = fixes_sources[fixes_source_idx.min(1)];
							let mut rl2 = GitHubRateLimit::default();
							let fixes_list = fetch_releases(owner_f, repo_f, &mut rl2).await.unwrap_or_default();
							if !fixes_list.is_empty() {
								let rel = fixes_list[fixes_release_idx.min(fixes_list.len()-1)].clone();
								let base = exec_dir.clone();
								let result = install_fixes_from_release(&rel, &base, Some(crate::app::DEFAULT_IGNORE_PATTERNS), |m,p| { let scaled = 60 + ((p as u16 * 25) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
								if result.is_ok() {
									let rel_name = rel.name.unwrap_or_else(|| rel.tag_name.unwrap_or_default());
									settings.installed_fixes_version = Some(rel_name);
								}
							}
							let patch_sources: [(&str, &str); 3] = [("sambow23", "SourceRTXTweaks"), ("BlueAmulet", "SourceRTXTweaks"), ("Xenthio", "SourceRTXTweaks")];
							let (owner_p, repo_p) = patch_sources[patch_source_idx.min(2)];
							let base = exec_dir.clone();
							let result = apply_patches_from_repo(owner_p, repo_p, "applypatch.py", &base, |m,p| { let scaled = 85 + ((p as u16 * 15) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled.min(99) }); }).await;
							if result.is_ok() {
								let patch_info = format!("{}/{}", owner_p, repo_p);
								settings.installed_patches_commit = Some(patch_info);
							}
							// Save settings with all version information
							let _ = settings_store.save(&settings);
							let _ = tx.send(JobProgress { message: "Quick install complete".into(), percent: 100 });
						});
					});
				}
			}
		}
	});
}


