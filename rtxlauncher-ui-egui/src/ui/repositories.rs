use eframe::egui;
use rtxlauncher_core::{GitHubRelease, JobProgress, fetch_releases, GitHubRateLimit, install_remix_from_release, install_fixes_from_release, apply_patches_from_repo};

pub struct RepositoriesState {
	pub is_running: bool,
	pub current_job: Option<std::sync::mpsc::Receiver<JobProgress>>,
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

impl Default for RepositoriesState {
	fn default() -> Self {
		Self {
			is_running: false,
			current_job: None,
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

impl RepositoriesState {
	pub fn append_log(&mut self, msg: &str) { if !self.log.is_empty() { self.log.push('\n'); } self.log.push_str(msg); }
	pub fn poll_job(&mut self) {
		if self.current_job.is_none() { return; }
		let mut finished = false;
		if let Some(rx) = self.current_job.take() {
			while let Ok(p) = rx.try_recv() {
				self.append_log(&p.message);
				if p.percent >= 100 { self.is_running = false; finished = true; }
			}
			if !finished { self.current_job = Some(rx); }
		}
	}
}

pub fn render_repositories_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	// Poll and kick off fetches without holding a long borrow
	{
		let st = &mut app.repositories;
		st.poll_job();
		if !st.remix_loading && st.remix_releases.is_empty() { start_fetch_releases(true, st); }
		if !st.fixes_loading && st.fixes_releases.is_empty() { start_fetch_releases(false, st); }
	}

	ui.heading("Repositories");
	ui.separator();

	// Base Game Updates header with deferred app mutation
	let mut trigger_update = false;
	{
		let st = &mut app.repositories;
		ui.group(|ui| {
			ui.heading("Base Game Updates");
			if ui.add_enabled(!st.is_running, egui::Button::new("Update Base Game")).clicked() { trigger_update = true; }
		});
	}
	if trigger_update { app.prepare_update_dialog(); app.show_update_dialog = true; }

	// Remix section
	{
		let st = &mut app.repositories;
		ui.group(|ui| {
			ui.heading("NVIDIA RTX Remix");
			let remix_sources: [(&str, &str, &str); 2] = [
				("sambow23/dxvk-remix-gmod", "sambow23", "dxvk-remix-gmod"),
				("(OFFICIAL) NVIDIAGameWorks/rtx-remix", "NVIDIAGameWorks", "rtx-remix"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_salt("remix-source").selected_text(remix_sources[st.remix_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in remix_sources.iter().enumerate() {
						if ui.selectable_label(st.remix_source_idx == i, *label).clicked() { st.remix_source_idx = i; start_fetch_releases(true, st); }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Version");
				let label = |r: &GitHubRelease| r.name.clone().unwrap_or_else(|| r.tag_name.clone().unwrap_or_default());
				let selected_text = if st.remix_releases.is_empty() { if st.remix_loading { "Loading...".to_string() } else { "No releases".to_string() } } else { label(&st.remix_releases[st.remix_release_idx.min(st.remix_releases.len()-1)]) };
				egui::ComboBox::from_id_salt("remix-version").selected_text(selected_text).show_ui(ui, |ui| {
					for (i, r) in st.remix_releases.iter().enumerate() {
						let text = label(r);
						if ui.selectable_label(st.remix_release_idx == i, text).clicked() { st.remix_release_idx = i; }
					}
				});
				if ui.add_enabled(!st.is_running && !st.remix_releases.is_empty(), egui::Button::new("Install/Update")).clicked() {
					let rel = st.remix_releases[st.remix_release_idx].clone();
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					st.current_job = Some(rx);
					st.is_running = true;
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
							let _ = install_remix_from_release(&rel, &base, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
						});
					});
				}
			});
		});
	}

	ui.add_space(8.0);

	// Fixes section
	{
		let st = &mut app.repositories;
		ui.group(|ui| {
			ui.heading("Fixes Package");
			let fixes_sources: [(&str, &str, &str); 2] = [
				("Xenthio/gmod-rtx-fixes-2 (Any)", "Xenthio", "gmod-rtx-fixes-2"),
				("Xenthio/RTXFixes (gmod_main)", "Xenthio", "RTXFixes"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_salt("fixes-source").selected_text(fixes_sources[st.fixes_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in fixes_sources.iter().enumerate() {
						if ui.selectable_label(st.fixes_source_idx == i, *label).clicked() { st.fixes_source_idx = i; start_fetch_releases(false, st); }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Version");
				let label = |r: &GitHubRelease| r.name.clone().unwrap_or_else(|| r.tag_name.clone().unwrap_or_default());
				let selected_text = if st.fixes_releases.is_empty() { if st.fixes_loading { "Loading...".to_string() } else { "No packages".to_string() } } else { label(&st.fixes_releases[st.fixes_release_idx.min(st.fixes_releases.len()-1)]) };
				egui::ComboBox::from_id_salt("fixes-version").selected_text(selected_text).show_ui(ui, |ui| {
					for (i, r) in st.fixes_releases.iter().enumerate() {
						let text = label(r);
						if ui.selectable_label(st.fixes_release_idx == i, text).clicked() { st.fixes_release_idx = i; }
					}
				});
				if ui.add_enabled(!st.is_running && !st.fixes_releases.is_empty(), egui::Button::new("Install/Update")).clicked() {
					let rel = st.fixes_releases[st.fixes_release_idx].clone();
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					st.current_job = Some(rx);
					st.is_running = true;
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
							let _ = install_fixes_from_release(&rel, &base, Some(crate::app::DEFAULT_IGNORE_PATTERNS), |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
						});
					});
				}
			});
		});
	}

	ui.add_space(8.0);

	// Patches section
	{
		let st = &mut app.repositories;
		ui.group(|ui| {
			ui.heading("Binary Patches");
			let patch_sources: [(&str, &str, &str); 3] = [
				("sambow23/SourceRTXTweaks", "sambow23", "SourceRTXTweaks"),
				("BlueAmulet/SourceRTXTweaks", "BlueAmulet", "SourceRTXTweaks"),
				("Xenthio/SourceRTXTweaks", "Xenthio", "SourceRTXTweaks"),
			];
			ui.horizontal(|ui| {
				ui.label("Source");
				egui::ComboBox::from_id_salt("patch-source").selected_text(patch_sources[st.patch_source_idx].0).show_ui(ui, |ui| {
					for (i, (label, _, _)) in patch_sources.iter().enumerate() {
						if ui.selectable_label(st.patch_source_idx == i, *label).clicked() { st.patch_source_idx = i; }
					}
				});
			});
			ui.horizontal(|ui| {
				ui.label("Action");
				if ui.add_enabled(!st.is_running, egui::Button::new("Apply Patches")).clicked() {
					let (owner, repo) = { let s = patch_sources[st.patch_source_idx]; (s.1.to_string(), s.2.to_string()) };
					let (tx, rx) = std::sync::mpsc::channel::<JobProgress>();
					st.current_job = Some(rx);
					st.is_running = true;
					let install_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
					std::thread::spawn(move || {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async move {
							let _ = apply_patches_from_repo(&owner, &repo, "applypatch.py", &install_dir, |m,p| { let _ = tx.send(JobProgress { message: m.to_string(), percent: p }); }).await;
						});
					});
				}
			});
		});
	}

	ui.separator();
	{
		let st = &mut app.repositories;
		let avail = ui.available_size();
		let height = avail.y.max(200.0);
		egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false,false]).max_height(height).show(ui, |ui| {
			ui.set_min_height(height);
			ui.monospace(&st.log);
		});
		if let Some(rx) = st.remix_rx.take() { if let Ok(list) = rx.try_recv() { st.remix_releases = list; st.remix_release_idx = 0; st.remix_loading = false; } else { st.remix_rx = Some(rx); } }
		if let Some(rx) = st.fixes_rx.take() { if let Ok(list) = rx.try_recv() { st.fixes_releases = list; st.fixes_release_idx = 0; st.fixes_loading = false; } else { st.fixes_rx = Some(rx); } }
	}
}

fn start_fetch_releases(remix: bool, st: &mut RepositoriesState) {
	let (owner, repo) = if remix {
		match st.remix_source_idx { 0 => ("sambow23", "dxvk-remix-gmod"), _ => ("NVIDIAGameWorks", "rtx-remix") }
	} else {
		match st.fixes_source_idx { 0 => ("Xenthio", "gmod-rtx-fixes-2"), _ => ("Xenthio", "RTXFixes") }
	};
	let (tx, rx) = std::sync::mpsc::channel::<Vec<GitHubRelease>>();
	if remix { st.remix_rx = Some(rx); st.remix_loading = true; } else { st.fixes_rx = Some(rx); st.fixes_loading = true; }
	std::thread::spawn(move || {
		let rt = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async move {
			let mut rl = GitHubRateLimit::default();
			let list = fetch_releases(owner, repo, &mut rl).await.unwrap_or_default();
			let _ = tx.send(list);
		});
	});
}


