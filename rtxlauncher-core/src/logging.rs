use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_appender::{rolling, non_blocking::WorkerGuard};
use once_cell::sync::OnceCell;
use std::fs;

static INIT: OnceCell<()> = OnceCell::new();
static FILE_GUARD: OnceCell<WorkerGuard> = OnceCell::new();

pub fn init_logging() {
    let _ = INIT.get_or_init(|| {
        let _ = fs::create_dir_all("logs");
        let file_appender = rolling::daily("logs", "rtxlauncher.log");
        let (nb_file, guard) = tracing_appender::non_blocking(file_appender);
        let _ = FILE_GUARD.set(guard); // keep guard alive for program lifetime

        // Console layer
        let console_layer = fmt::layer().with_target(false);
        // File layer
        let file_layer = fmt::layer().with_writer(nb_file).with_target(false);

        let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        tracing_subscriber::registry()
            .with(env)
            .with(console_layer)
            .with(file_layer)
            .init();
    });
}

/// Emit throttled progress updates to the UI and tracing logs.
/// Ensures messages with the same prefix (e.g., "Downloading:") are not emitted more than once every `min_interval_ms`.
pub struct ProgressThrottle {
    last_msg: String,
    last_instant: std::time::Instant,
    min_interval: std::time::Duration,
}

impl ProgressThrottle {
    pub fn new(min_interval_ms: u64) -> Self {
        Self { last_msg: String::new(), last_instant: std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3600)).unwrap_or_else(std::time::Instant::now), min_interval: std::time::Duration::from_millis(min_interval_ms) }
    }

    pub fn emit(&mut self, prefix: &str, msg: String, pct: u8, mut ui_progress: impl FnMut(&str, u8)) {
        let now = std::time::Instant::now();
        let same_prefix = self.last_msg.starts_with(prefix) && msg.starts_with(prefix);
        if !same_prefix || now.duration_since(self.last_instant) >= self.min_interval {
            ui_progress(&msg, pct);
            tracing::info!(target: "progress", "{}", msg);
            self.last_msg = msg;
            self.last_instant = now;
        }
    }
}


