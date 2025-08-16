use eframe::egui;
use rtxlauncher_core::{JobProgress, InstallPlan, detect_gmod_install_folder, perform_basic_install, GitHubRateLimit, fetch_releases, install_remix_from_release, install_fixes_from_release, GitHubRelease, apply_patches_from_repo};

pub struct InstallState {
	pub is_running: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
	pub progress: u8,
	pub log: String,
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
			log: String::new(),
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
	pub fn append_log(&mut self, msg: &str) { if !self.log.is_empty() { self.log.push('\n'); } self.log.push_str(msg); }
	pub fn poll_job(&mut self) {
		if self.current_job.is_none() { return; }
		let mut finished = false;
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				self.progress = p.percent;
				self.append_log(&p.message);
				if p.percent >= 100 { self.is_running = false; finished = true; }
			}
			if !finished { self.current_job = Some(rx); }
		}
	}
}

pub fn render_install_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	let st = &mut app.install;
	st.poll_job();
	ui.heading("Install");
	ui.add_enabled_ui(!st.is_running, |ui| {
		if ui.button("Quick Install").clicked() {
			let vanilla_opt = app.settings.manually_specified_install_path.clone().or_else(|| detect_gmod_install_folder().map(|p| p.display().to_string()));
			if let Some(vanilla) = vanilla_opt {
				if let Ok(exec_dir) = std::env::current_exe().map(|p| p.parent().unwrap().to_path_buf()) {
					let plan = InstallPlan { vanilla: std::path::PathBuf::from(vanilla), rtx: exec_dir.clone() };
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					st.current_job = Some(rx);
					st.is_running = true;
					let remix_source_idx = st.remix_source_idx;
					let remix_release_idx = st.remix_release_idx;
					let fixes_source_idx = st.fixes_source_idx;
					let fixes_release_idx = st.fixes_release_idx;
					let patch_source_idx = st.patch_source_idx;
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
								let _ = install_remix_from_release(&rel, &base, |m,p| { let scaled = 25 + ((p as u16 * 35) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
							}
							let fixes_sources: [(&str, &str); 2] = [("Xenthio", "gmod-rtx-fixes-2"), ("Xenthio", "RTXFixes")];
							let (owner_f, repo_f) = fixes_sources[fixes_source_idx.min(1)];
							let mut rl2 = GitHubRateLimit::default();
							let fixes_list = fetch_releases(owner_f, repo_f, &mut rl2).await.unwrap_or_default();
							if !fixes_list.is_empty() {
								let rel = fixes_list[fixes_release_idx.min(fixes_list.len()-1)].clone();
								let base = exec_dir.clone();
								let _ = install_fixes_from_release(&rel, &base, Some(crate::app::DEFAULT_IGNORE_PATTERNS), |m,p| { let scaled = 60 + ((p as u16 * 25) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled }); }).await;
							}
							let patch_sources: [(&str, &str); 3] = [("sambow23", "SourceRTXTweaks"), ("BlueAmulet", "SourceRTXTweaks"), ("Xenthio", "SourceRTXTweaks")];
							let (owner_p, repo_p) = patch_sources[patch_source_idx.min(2)];
							let base = exec_dir.clone();
							let _ = apply_patches_from_repo(owner_p, repo_p, "applypatch.py", &base, |m,p| { let scaled = 85 + ((p as u16 * 15) / 100) as u8; let _ = tx.send(JobProgress { message: m.to_string(), percent: scaled.min(99) }); }).await;
							let _ = tx.send(JobProgress { message: "Quick install complete".into(), percent: 100 });
						});
					});
				}
			}
		}
	});
	ui.separator();
	// Logs area with controls
	ui.horizontal(|ui| {
		ui.label("Logs:");
		if ui.small_button("Copy").clicked() { ui.output_mut(|o| o.copied_text = st.log.clone()); }
		if ui.small_button("Clear").clicked() { st.log.clear(); }
	});
	let avail = ui.available_size();
	let height = avail.y.max(200.0);
	egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false,false]).max_height(height).show(ui, |ui| {
		ui.set_min_height(height);
		ui.monospace(&st.log);
	});
}


