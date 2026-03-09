use std::fs;
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::Registry;

const LOG_DIR: &str = "lazyagent";
const LOG_PREFIX: &str = "lazyagent.log";
const ERROR_LOG_PREFIX: &str = "lazyagent-error.log";
const RETENTION_DAYS: u64 = 7;

/// Initialize file-based logging with two log files:
/// - `lazyagent.log.*`: all logs (default info, override via `LAZYAGENT_LOG`)
/// - `lazyagent-error.log.*`: error-only logs (always ERROR level)
///
/// Returns guards that must be held until shutdown.
pub fn init_logging() -> Vec<WorkerGuard> {
    let Some(log_dir) = log_dir() else {
        return vec![];
    };
    if fs::create_dir_all(&log_dir).is_err() {
        return vec![];
    }

    cleanup_old_logs(&log_dir, LOG_PREFIX);
    cleanup_old_logs(&log_dir, ERROR_LOG_PREFIX);

    // All-level log: LAZYAGENT_LOG env var, default to "info"
    let filter = EnvFilter::try_from_env("LAZYAGENT_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let all_appender = tracing_appender::rolling::daily(&log_dir, LOG_PREFIX);
    let (all_writer, all_guard) = tracing_appender::non_blocking(all_appender);

    let all_layer = fmt::layer()
        .with_writer(all_writer)
        .with_ansi(false)
        .with_filter(filter);

    // Error-only log
    let error_appender = tracing_appender::rolling::daily(&log_dir, ERROR_LOG_PREFIX);
    let (error_writer, error_guard) = tracing_appender::non_blocking(error_appender);

    let error_layer = fmt::layer()
        .with_writer(error_writer)
        .with_ansi(false)
        .with_filter(tracing_subscriber::filter::LevelFilter::ERROR);

    Registry::default()
        .with(all_layer)
        .with(error_layer)
        .init();

    vec![all_guard, error_guard]
}

fn log_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .map(|d| d.join(LOG_DIR))
}

/// Remove log files older than retention period matching the given prefix.
fn cleanup_old_logs(dir: &std::path::Path, prefix: &str) {
    cleanup_old_logs_with_retention(dir, prefix, RETENTION_DAYS);
}

fn cleanup_old_logs_with_retention(dir: &std::path::Path, prefix: &str, retention_days: u64) {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(retention_days * 24 * 60 * 60);

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn make_temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lazyagent-test-{}-{}",
            std::process::id(),
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn create_file(dir: &PathBuf, name: &str) {
        let path = dir.join(name);
        fs::File::create(&path).unwrap().write_all(b"log").unwrap();
    }

    fn set_mtime_days_ago(dir: &PathBuf, name: &str, days: u64) {
        let path = dir.join(name);
        let mtime = SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60);
        let times = fs::FileTimes::new().set_modified(mtime);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(times)
            .unwrap();
    }

    #[test]
    fn test_log_dir_returns_some() {
        let dir = log_dir();
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!(dir.to_string_lossy().contains("lazyagent"));
    }

    #[test]
    fn test_cleanup_deletes_old_log_files() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent.log.2026-01-01");
        set_mtime_days_ago(&dir, "lazyagent.log.2026-01-01", 30);

        cleanup_old_logs_with_retention(&dir, LOG_PREFIX, 7);

        assert!(!dir.join("lazyagent.log.2026-01-01").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_keeps_recent_log_files() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent.log.2026-03-09");

        cleanup_old_logs_with_retention(&dir, LOG_PREFIX, 7);

        assert!(dir.join("lazyagent.log.2026-03-09").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_skips_non_matching_files() {
        let dir = make_temp_dir();
        create_file(&dir, "other.txt");
        set_mtime_days_ago(&dir, "other.txt", 30);

        cleanup_old_logs_with_retention(&dir, LOG_PREFIX, 7);

        assert!(dir.join("other.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_handles_nonexistent_dir() {
        let dir = PathBuf::from("/tmp/lazyagent-nonexistent-dir-abc123");
        cleanup_old_logs_with_retention(&dir, LOG_PREFIX, 7);
    }

    #[test]
    fn test_cleanup_handles_empty_dir() {
        let dir = make_temp_dir();
        cleanup_old_logs_with_retention(&dir, LOG_PREFIX, 7);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_deletes_old_error_log_files() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent-error.log.2026-01-01");
        set_mtime_days_ago(&dir, "lazyagent-error.log.2026-01-01", 30);

        cleanup_old_logs_with_retention(&dir, ERROR_LOG_PREFIX, 7);

        assert!(!dir.join("lazyagent-error.log.2026-01-01").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_keeps_recent_error_log_files() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent-error.log.2026-03-09");

        cleanup_old_logs_with_retention(&dir, ERROR_LOG_PREFIX, 7);

        assert!(dir.join("lazyagent-error.log.2026-03-09").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_error_prefix_does_not_match_normal_prefix() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent.log.2026-01-01");
        set_mtime_days_ago(&dir, "lazyagent.log.2026-01-01", 30);

        // Cleaning with error prefix should NOT delete normal log files
        cleanup_old_logs_with_retention(&dir, ERROR_LOG_PREFIX, 7);

        assert!(dir.join("lazyagent.log.2026-01-01").exists());
        fs::remove_dir_all(&dir).ok();
    }
}
