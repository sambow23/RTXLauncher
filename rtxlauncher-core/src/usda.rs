use anyhow::Result;
use reqwest::Client;
use std::path::Path;
use zip::ZipArchive;
use std::io::Cursor;
use futures_util::StreamExt;
use std::time::Duration;
use tracing::info;
use crate::logging::ProgressThrottle;

pub async fn apply_usda_fixes(game_install_path: &Path, remix_mod_folder: &str, mut progress: impl FnMut(&str, u8)) -> Result<bool> {
	if remix_mod_folder != "hl2rtx" { return Ok(true); }
	let url = "https://github.com/sambow23/rtx-usda-fixes/archive/refs/heads/main.zip";
	progress("Downloading USDA fixes", 10);

	info!("USDA download start: {}", url);
	let client = match Client::builder().timeout(Duration::from_secs(300)).build() {
		Ok(c) => c,
		Err(e) => { progress(&format!("USDA error: {}", e), 100); info!("USDA client error: {}", e); return Ok(false); }
	};
	let resp = match client.get(url).header("User-Agent", "RTXLauncher-RS").send().await {
		Ok(r) => r,
		Err(e) => { progress(&format!("USDA error: {}", e), 100); info!("USDA request error: {}", e); return Ok(false); }
	};
	let status = resp.status();
	if !status.is_success() {
		progress(&format!("HTTP error: {}", status), 100);
		info!("USDA HTTP error: {}", status);
		return Ok(false);
	}
	let total = resp.content_length().unwrap_or(0);
	info!("USDA content_length: {} bytes", total);
	let mut stream = resp.bytes_stream();
	let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
	let mut downloaded: u64 = 0;
	let mut chunks = 0u64;
	let mut throttler = ProgressThrottle::new(150);
	while let Some(chunk_res) = stream.next().await {
		let chunk = match chunk_res { Ok(c) => c, Err(e) => { progress(&format!("USDA stream error: {}", e), 100); info!("USDA stream error: {}", e); return Ok(false); } };
		downloaded += chunk.len() as u64;
		buf.extend_from_slice(&chunk);
		chunks += 1;
		if total > 0 {
			let pct = 10 + ((downloaded as f32 / total as f32) * 60.0) as u8;
			let msg = format!("Downloading: {}/{} MB", downloaded/1_048_576, total/1_048_576);
			throttler.emit("Downloading:", msg, pct.min(70), |m,p| progress(m,p));
		}
		if chunks % 32 == 0 { info!("USDA downloaded {} bytes ({} chunks)", downloaded, chunks); }
	}
	info!("USDA download complete: {} bytes ({} chunks)", downloaded, chunks);

	// Write to temp for debugging
	if let Ok(tmpdir) = std::env::temp_dir().canonicalize() {
		let tmpzip = tmpdir.join("rtx_usda_fixes.zip");
		if let Ok(mut f) = std::fs::File::create(&tmpzip) {
			let _ = std::io::Write::write_all(&mut f, &buf);
			info!("USDA zip saved to {} ({} bytes)", tmpzip.display(), buf.len());
		}
	}

	// Build two independent archives from the same buffer so counting doesn't affect extraction
	let mut zip_count = match ZipArchive::new(Cursor::new(buf.clone())) {
		Ok(z) => z,
		Err(e) => { progress(&format!("USDA zip open error: {}", e), 100); info!("USDA zip open error: {}", e); return Ok(false); }
	};
	let dest = game_install_path.join("rtx-remix").join("mods").join(remix_mod_folder);
	if !dest.exists() {
		if let Err(e) = std::fs::create_dir_all(&dest) {
			progress(&format!("USDA destination missing and could not be created: {}", e), 100);
			info!("USDA dest create error: {}", e);
			return Ok(false);
		}
	}

	// Count total usda files to copy for progress
	let mut total_usda = 0u32;
	for i in 0..zip_count.len() {
		let f = zip_count.by_index(i)?;
		let name = f.name().to_string();
		if name.ends_with(".usda") { total_usda += 1; }
	}

	if total_usda == 0 {
		progress("No USDA files found; skipping", 100);
		info!("USDA: no .usda files found in archive");
		return Ok(true);
	}

	// Extract from a fresh archive instance
	let mut zip = match ZipArchive::new(Cursor::new(buf)) {
		Ok(z) => z,
		Err(e) => { progress(&format!("USDA zip reopen error: {}", e), 100); info!("USDA zip reopen error: {}", e); return Ok(false); }
	};

	let mut copied = 0u32;
	for i in 0..zip.len() {
		let mut f = zip.by_index(i)?;
		let name = f.name().to_string();
		if name.ends_with(".usda") {
			let base = name.rsplit('/').next().unwrap_or(&name);
			let path = dest.join(base);
			if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
			let mut out = match std::fs::File::create(&path) { Ok(f) => f, Err(e) => { progress(&format!("USDA write error: {}", e), 100); info!("USDA write error: {}", e); return Ok(false); } };
			if let Err(e) = std::io::copy(&mut f, &mut out) { progress(&format!("USDA copy error: {}", e), 100); info!("USDA copy error: {}", e); return Ok(false); }
			copied += 1;
			if total_usda > 0 {
				let pct = 70 + ((copied as f32 / total_usda as f32) * 30.0) as u8;
				progress(&format!("Copied {}/{} USDA files", copied, total_usda), pct.min(100));
			}
		}
	}
	progress(&format!("Copied {} USDA files", copied), 100);
	Ok(true)
}


