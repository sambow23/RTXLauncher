fn main() {
    // Embed short git commit hash if available
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"]).output()
        .ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", hash);
}


