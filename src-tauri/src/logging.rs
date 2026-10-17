#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::Once;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::errors::AppError;

static PANIC_HOOK_ONCE: Once = Once::new();

/// Keeps the background log writer alive until application shutdown.
pub struct LogGuard {
    _worker_guard: WorkerGuard,
}

/// Initializes structured tracing with local daily rotating log files.
pub fn init_logging(log_dir: &Path) -> Result<LogGuard, AppError> {
    std::fs::create_dir_all(log_dir)
        .map_err(|err| AppError::from_io("create log directory", err))?;

    let file_appender = tracing_appender::rolling::daily(log_dir, "sortdeck.log");
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sortdeck_lib=info,warn"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(non_blocking);

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .try_init();

    Ok(LogGuard {
        _worker_guard: worker_guard,
    })
}

/// Installs a global panic hook that records structured diagnostics via `tracing`.
pub fn install_panic_hook() {
    PANIC_HOOK_ONCE.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let location = panic_info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "unknown".to_owned());

            let payload = if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
                (*msg).to_owned()
            } else if let Some(msg) = panic_info.payload().downcast_ref::<String>() {
                msg.clone()
            } else {
                "non-string panic payload".to_owned()
            };

            tracing::error!(location = %location, panic_message = %payload, "unhandled panic");
            default_hook(panic_info);
        }));
    });
}
