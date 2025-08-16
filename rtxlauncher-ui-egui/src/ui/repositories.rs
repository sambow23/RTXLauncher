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
				if st.remix_loading { ui.add(egui::Spinner::new()); }
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
			// details panel
			if let Some(rel) = st.remix_releases.get(st.remix_release_idx) {
				ui.separator();
				let name = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
				let prerelease = rel.prerelease.unwrap_or(false);
				ui.horizontal(|ui| {
					ui.label(format!("Selected: {}", name));
					if prerelease { ui.colored_label(egui::Color32::YELLOW, "pre-release"); }
					let installed = app.settings.installed_remix_version.clone().unwrap_or_default();
					if !installed.is_empty() {
						let up_to_date = installed == name;
						let col = if up_to_date { egui::Color32::from_rgb(0,200,0) } else { egui::Color32::from_rgb(200,140,0) };
						ui.colored_label(col, if up_to_date { "Up to date" } else { "Update available" });
						ui.label(format!("Installed: {}", installed));
					}
				});
				if let Some(body) = &rel.body {
					egui::ScrollArea::vertical().id_salt("remix-md").max_height(260.0).show(ui, |ui| { render_simple_markdown(ui, body); });
				}
			}
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
				if st.fixes_loading { ui.add(egui::Spinner::new()); }
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
			// details panel
			if let Some(rel) = st.fixes_releases.get(st.fixes_release_idx) {
				ui.separator();
				let name = rel.name.clone().unwrap_or_else(|| rel.tag_name.clone().unwrap_or_default());
				ui.horizontal(|ui| {
					ui.label(format!("Selected: {}", name));
					let installed = app.settings.installed_fixes_version.clone().unwrap_or_default();
					if !installed.is_empty() {
						let up_to_date = installed == name;
						let col = if up_to_date { egui::Color32::from_rgb(0,200,0) } else { egui::Color32::from_rgb(200,140,0) };
						ui.colored_label(col, if up_to_date { "Up to date" } else { "Update available" });
						ui.label(format!("Installed: {}", installed));
					}
				});
				if let Some(body) = &rel.body {
					egui::ScrollArea::vertical().id_salt("fixes-md").max_height(260.0).show(ui, |ui| { render_simple_markdown(ui, body); });
				}
			}
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

// Minimal markdown renderer (headings h1..h6, bullet lists, code blocks, simple links & inline code)
fn render_simple_markdown(ui: &mut egui::Ui, text: &str) {
	let mut in_code = false;
	for raw_line in text.lines() {
		let line = raw_line.trim_end();
		if line.starts_with("```") { in_code = !in_code; continue; }
		if in_code { ui.monospace(line); continue; }
		// headings h6..h1 (render inline so links/bold work inside)
		if let Some(rest) = line.strip_prefix("###### ") { render_inline_with_heading(ui, rest, true); continue; }
		if let Some(rest) = line.strip_prefix("##### ") { render_inline_with_heading(ui, rest, true); continue; }
		if let Some(rest) = line.strip_prefix("#### ") { render_inline_with_heading(ui, rest, true); continue; }
		if let Some(rest) = line.strip_prefix("### ") { render_inline_with_heading(ui, rest, true); continue; }
		if let Some(rest) = line.strip_prefix("## ") { render_inline_with_heading(ui, rest, true); continue; }
		if let Some(rest) = line.strip_prefix("# ") { render_inline_with_heading(ui, rest, true); continue; }
		// bullets
		if let Some(rest) = line.strip_prefix("- ") { ui.horizontal_wrapped(|ui| { ui.label("•"); render_inline_with_heading(ui, rest, false); }); continue; }
		if let Some(rest) = line.strip_prefix("* ") { ui.horizontal_wrapped(|ui| { ui.label("•"); render_inline_with_heading(ui, rest, false); }); continue; }
		// plain
		if line.is_empty() { ui.add_space(4.0); } else { render_inline_with_heading(ui, line, false); }
	}
}

// inline renderer with optional heading styling: supports **bold**, `code`, and [label](url)
fn render_inline_with_heading(ui: &mut egui::Ui, line: &str, heading: bool) {
	#[derive(Debug)]
	enum Seg { Text(String, bool), Code(String), Link { label: String, url: String, bold: bool } }
	let mut segs: Vec<Seg> = Vec::new();
	let mut bold = false;
	let mut code = false;
	let mut buf = String::new();
	let mut i = 0usize;
	let chars: Vec<char> = line.chars().collect();
	while i < chars.len() {
		// toggle bold on **
		if !code && i + 1 < chars.len() && chars[i] == '*' && chars[i+1] == '*' {
			if !buf.is_empty() { segs.push(Seg::Text(std::mem::take(&mut buf), bold)); }
			bold = !bold; i += 2; continue;
		}
		// inline link [text](url)
		if !code && chars[i] == '[' {
			let rest: String = chars[i..].iter().collect();
			if let Some(close_br) = rest.find(']') {
				let after = &rest[close_br+1..];
				if after.starts_with('(') {
					if let Some(close_paren) = after[1..].find(')') {
						if !buf.is_empty() { segs.push(Seg::Text(std::mem::take(&mut buf), bold)); }
						let mut label = rest[1..close_br].trim().to_string();
						if label.starts_with('`') && label.ends_with('`') && label.len() >= 2 { label = label[1..label.len()-1].to_string(); }
						let url = &after[1..1+close_paren];
						segs.push(Seg::Link { label, url: url.to_string(), bold });
						// advance i by consumed chars
						i += 1 + close_br + 1 + 1 + close_paren + 1;
						continue;
					}
				}
			}
		}
		// inline code with backticks
		if chars[i] == '`' {
			if code { segs.push(Seg::Code(std::mem::take(&mut buf))); code = false; }
			else { if !buf.is_empty() { segs.push(Seg::Text(std::mem::take(&mut buf), bold)); } code = true; }
			i += 1; continue;
		}
		// normal char
		buf.push(chars[i]);
		i += 1;
	}
	if !buf.is_empty() { if code { segs.push(Seg::Code(buf)); } else { segs.push(Seg::Text(buf, bold)); } }
	ui.horizontal_wrapped(|ui| {
		for seg in segs.into_iter() {
			match seg {
				Seg::Text(s, b) => {
					let mut t = egui::RichText::new(s);
					if b { t = t.strong(); }
					if heading { t = t.heading(); }
					ui.label(t);
				}
				Seg::Code(s) => { let mut t = egui::RichText::new(s).code(); if heading { t = t.heading(); } ui.label(t); }
				Seg::Link { label, url, bold: b } => {
					let mut text = egui::RichText::new(label);
					if b { text = text.strong(); }
					if heading { text = text.heading(); }
					ui.add(egui::widgets::Hyperlink::from_label_and_url(text, url));
				}
			}
		}
	});
}

// Backwards-compat shim
fn render_inline(ui: &mut egui::Ui, line: &str) { render_inline_with_heading(ui, line, false); }


