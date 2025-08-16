use anyhow::{Result, Context};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::Duration};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitHubRelease {
    pub name: Option<String>,
    pub tag_name: Option<String>,
    pub published_at: Option<String>,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Default)]
pub struct GitHubRateLimit {
    pub limit: i32,
    pub remaining: i32,
    pub reset_unix: i64,
}

fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "rtxlauncher", "rtxlauncher")
        .ok_or_else(|| anyhow::anyhow!("project dirs"))?;
    let dir = dirs.cache_dir().join("github");
    fs::create_dir_all(&dir).ok();
    Ok(dir)
}

fn token_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "rtxlauncher", "rtxlauncher")
        .ok_or_else(|| anyhow::anyhow!("project dirs"))?;
    let dir = dirs.config_dir();
    fs::create_dir_all(dir).ok();
    Ok(dir.join("github_token.dat"))
}

pub fn set_personal_access_token(token: Option<String>) -> Result<()> {
    let path = token_path()?;
    match token {
        Some(t) if !t.is_empty() => fs::write(path, t).context("write token")?,
        _ => { let _ = fs::remove_file(path); }
    }
    Ok(())
}

pub fn load_personal_access_token() -> Option<String> {
    let path = token_path().ok()?;
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn cache_is_valid(p: &PathBuf, ttl: Duration) -> bool {
    if let Ok(meta) = fs::metadata(p) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = modified.elapsed() { return elapsed < ttl; }
        }
    }
    false
}

pub async fn fetch_releases(owner: &str, repo: &str, rate_limit: &mut GitHubRateLimit) -> Result<Vec<GitHubRelease>> {
    let cache = cache_dir()?.join(format!("{}_{}_releases.json", owner, repo));
    let ttl = Duration::from_secs(8 * 60);
    if cache_is_valid(&cache, ttl) {
        if let Ok(text) = fs::read_to_string(&cache) {
            if let Ok(v) = serde_json::from_str::<Vec<GitHubRelease>>(&text) { return Ok(v); }
        }
    }

    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases");
    info!("GitHub fetch: {}", url);
    let mut req = client.get(&url)
        .header("User-Agent", "RTXLauncher-RS")
        .header("Accept", "application/vnd.github.v3+json");
    if let Some(token) = load_personal_access_token() {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await?;

    // capture rate limit
    if let Some(v) = resp.headers().get("X-RateLimit-Limit") { rate_limit.limit = v.to_str().unwrap_or("0").parse().unwrap_or(0); }
    if let Some(v) = resp.headers().get("X-RateLimit-Remaining") { rate_limit.remaining = v.to_str().unwrap_or("0").parse().unwrap_or(0); }
    if let Some(v) = resp.headers().get("X-RateLimit-Reset") { rate_limit.reset_unix = v.to_str().unwrap_or("0").parse().unwrap_or(0); }

    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("GitHub API error: {}", status);
    }
    fs::write(&cache, &text).ok();
    let releases: Vec<GitHubRelease> = serde_json::from_str(&text)?;
    Ok(releases)
}


