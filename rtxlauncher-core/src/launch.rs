use crate::settings::AppSettings;
use std::path::PathBuf;
use std::process::Command;

fn split_args_quoted(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut quote_char: char = '\0';
    let mut escape = false;
    for ch in src.chars() {
        if escape {
            cur.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' => { escape = true; }
            '"' | '\'' => {
                if in_quotes {
                    if ch == quote_char { in_quotes = false; } else { cur.push(ch); }
                } else {
                    in_quotes = true; quote_char = ch;
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() { out.push(cur.clone()); cur.clear(); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

pub fn build_launch_args(settings: &AppSettings) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if settings.console_enabled { args.push("-console".into()); }
    // Always enforce DX level 90 as requested (two separate argv entries)
    args.push("-dxlevel".into());
    args.push("90".into());
    // D3D9Ex disable and windowing flags (each token separately)
    args.push("+mat_disable_d3d9ex".into()); args.push("1".into());
    args.push("-nod3d9ex".into());
    args.push("-windowed".into());
    args.push("-noborder".into());
    if let (Some(w), Some(h)) = (settings.width, settings.height) {
        if w > 0 && h > 0 {
            args.push("-w".into()); args.push(w.to_string());
            args.push("-h".into()); args.push(h.to_string());
        }
    }
    if !settings.load_workshop_addons { args.push("-noworkshop".into()); }
    if settings.disable_chromium { args.push("-nochromium".into()); }
    if settings.developer_mode { args.push("-dev".into()); }
    if settings.tools_mode { args.push("-tools".into()); }
    if let Some(custom) = &settings.custom_launch_options {
        let extra = split_args_quoted(custom);
        args.extend(extra);
    }
    args
}

pub fn launch_game(exe_path: PathBuf, settings: &AppSettings) -> std::io::Result<()> {
    let args = build_launch_args(settings);
    let mut cmd = Command::new(&exe_path);
    cmd.args(args);
    if let Some(dir) = exe_path.parent() { cmd.current_dir(dir); }
    let _ = cmd.spawn()?;
    Ok(())
}


