use std::fs;
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

const LOG_DIR: &str = "lazyagent";
const LOG_PREFIX: &str = "lazyagent.log";
const RETENTION_DAYS: u64 = 7;

/// Initialize file-based logging. Returns guard that must be held until shutdown.
/// Logging is controlled by `LAZYAGENT_LOG` env var (default: off).
/// Returns `None` if logging is disabled or setup fails.
pub fn init_logging() -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_env("LAZYAGENT_LOG").ok()?;

    let log_dir = log_dir()?;
    fs::create_dir_all(&log_dir).ok()?;

    cleanup_old_logs(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, LOG_PREFIX);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Some(guard)
}

fn log_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))
        .map(|d| d.join(LOG_DIR))
}

/// Remove log files older than retention period.
fn cleanup_old_logs(dir: &std::path::Path) {
    cleanup_old_logs_with_retention(dir, RETENTION_DAYS);
}

fn cleanup_old_logs_with_retention(dir: &std::path::Path, retention_days: u64) {
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
        if !name.starts_with(LOG_PREFIX) {
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

    fn make_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lazyagent-test-{}", std::process::id()));
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

        cleanup_old_logs_with_retention(&dir, 7);

        assert!(!dir.join("lazyagent.log.2026-01-01").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_keeps_recent_log_files() {
        let dir = make_temp_dir();
        create_file(&dir, "lazyagent.log.2026-03-09");
        // mtime is now (just created), so within retention

        cleanup_old_logs_with_retention(&dir, 7);

        assert!(dir.join("lazyagent.log.2026-03-09").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_skips_non_matching_files() {
        let dir = make_temp_dir();
        create_file(&dir, "other.txt");
        set_mtime_days_ago(&dir, "other.txt", 30);

        cleanup_old_logs_with_retention(&dir, 7);

        assert!(dir.join("other.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cleanup_handles_nonexistent_dir() {
        let dir = PathBuf::from("/tmp/lazyagent-nonexistent-dir-abc123");
        // Should not panic
        cleanup_old_logs_with_retention(&dir, 7);
    }

    #[test]
    fn test_cleanup_handles_empty_dir() {
        let dir = make_temp_dir();
        // Should not panic on empty dir
        cleanup_old_logs_with_retention(&dir, 7);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_init_logging_returns_none_without_env() {
        // LAZYAGENT_LOG not set → returns None
        std::env::remove_var("LAZYAGENT_LOG");
        let guard = init_logging();
        assert!(guard.is_none());
    }
}
