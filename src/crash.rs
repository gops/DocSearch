use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static CRASH_HANDLING: AtomicBool = AtomicBool::new(false);

/// Returns the path to the crash log file (~/.config/docsearch/crash.log).
pub fn crash_log_path() -> PathBuf {
    if let Ok(custom) = std::env::var("DOCSEARCH_CRASH_LOG") {
        return PathBuf::from(custom);
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config/docsearch/crash.log")
    } else {
        PathBuf::from("data/crash.log")
    }
}

/// Returns the path to the crash history file used for crash-loop throttling.
pub fn crash_history_path() -> PathBuf {
    if let Ok(custom) = std::env::var("DOCSEARCH_CRASH_HISTORY") {
        return PathBuf::from(custom);
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config/docsearch/.crash_history")
    } else {
        PathBuf::from("data/.crash_history")
    }
}

/// Installs the global panic hook to catch any unexpected crash, write logs, and auto-restart.
pub fn init_crash_handler() {
    // Ensure backtrace capture is enabled by default
    if std::env::var("RUST_BACKTRACE").is_err() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        // Run default hook so output also appears in standard logger if enabled
        default_hook(panic_info);

        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unknown");

        let payload_msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any> panic payload (no string message available)".to_string()
        };

        let location = panic_info.location().map(|loc| {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        });

        let backtrace = std::backtrace::Backtrace::force_capture();

        handle_crash(
            &format!("Panic on thread '{}': {}", thread_name, payload_msg),
            location.as_deref(),
            &backtrace,
        );
    }));
}

/// Core crash handler: logs report, checks crash-loop guard, and auto-restarts application.
pub fn handle_crash(reason: &str, location: Option<&str>, backtrace: &std::backtrace::Backtrace) {
    if CRASH_HANDLING.swap(true, Ordering::SeqCst) {
        // Prevent recursive panic in crash handler
        return;
    }

    let timestamp_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let iso_time = chrono::Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION");
    let clean_path = crate::tray::resolve_clean_exe_path();
    let original_args: Vec<String> = std::env::args().collect();

    let mut report = String::new();
    report.push_str("================================================================================\n");
    report.push_str("DOCSEARCH CRASH REPORT\n");
    report.push_str("================================================================================\n");
    report.push_str(&format!("Timestamp:     {}\n", iso_time));
    report.push_str(&format!("Version:       v{}\n", version));
    report.push_str(&format!("Binary Path:   {}\n", clean_path.display()));
    report.push_str(&format!("Arguments:     {:?}\n", original_args));
    report.push_str(&format!("Crash Reason:  {}\n", reason));
    if let Some(loc) = location {
        report.push_str(&format!("Location:      {}\n", loc));
    }
    report.push_str("\nStack Backtrace:\n");
    report.push_str(&format!("{:#}\n", backtrace));
    report.push_str("================================================================================\n");

    let log_path = crash_log_path();
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).write(true).truncate(true).open(&log_path) {
        let _ = file.write_all(report.as_bytes());
        let _ = file.sync_all();
    }

    eprintln!("\n{}", report);

    // Only auto-restart long-running GUI or daemon modes, NOT one-shot CLI commands
    let is_cli_subcommand = original_args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "search" | "add" | "list" | "reindex" | "duplicates" | "crash-log" | "--help" | "-h" | "--version" | "-V"
        )
    });

    if is_cli_subcommand {
        eprintln!("🚨 [Crash Handler] CLI command failed. Crash log saved to: {}", log_path.display());
        std::process::exit(101);
    }

    // Check crash loop protection (e.g. max 3 crashes within 30 seconds)
    let crash_count = record_crash_and_check_loop(timestamp_secs, &crash_history_path());

    if crash_count >= 3 {
        eprintln!("🚨 [Crash Recovery] Rapid crash loop detected ({} crashes in <30 seconds).", crash_count);
        eprintln!("🚨 [Crash Recovery] Halting auto-restart to protect system stability.");
        eprintln!("🚨 [Crash Recovery] Full crash report saved at: {}\n", log_path.display());
        std::process::exit(101);
    }

    eprintln!("🚨 [Crash Recovery] Auto-restarting DocSearch in 500ms...");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Prepare restart arguments with recovery notification flag
    let mut restart_args: Vec<String> = std::env::args().skip(1).collect();
    if !restart_args.iter().any(|a| a == "--recovered-from-crash") {
        restart_args.push("--recovered-from-crash".to_string());
    }

    match std::process::Command::new(&clean_path).args(&restart_args).spawn() {
        Ok(child) => {
            eprintln!("🚨 [Crash Recovery] Restarted DocSearch process (PID: {}). Exiting crashed process.", child.id());
        }
        Err(e) => {
            eprintln!("🚨 [Crash Recovery] Failed to auto-restart: {}", e);
        }
    }

    std::process::exit(101);
}

/// Reads the latest crash log if it exists.
pub fn get_latest_crash_log() -> Option<String> {
    let p = crash_log_path();
    if p.exists() {
        fs::read_to_string(&p).ok()
    } else {
        None
    }
}

/// Clears the crash log.
pub fn clear_crash_log() {
    let p = crash_log_path();
    let _ = fs::remove_file(&p);
}

/// Records a crash timestamp and returns the count of crashes in the last 30 seconds.
pub fn record_crash_and_check_loop(now_secs: u64, history_path: &PathBuf) -> usize {
    if let Some(parent) = history_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut timestamps = Vec::new();
    if let Ok(content) = fs::read_to_string(history_path) {
        for line in content.lines() {
            if let Ok(t) = line.trim().parse::<u64>() {
                if now_secs.saturating_sub(t) <= 30 {
                    timestamps.push(t);
                }
            }
        }
    }

    timestamps.push(now_secs);

    if let Ok(mut f) = OpenOptions::new().create(true).write(true).truncate(true).open(history_path) {
        for t in &timestamps {
            let _ = writeln!(f, "{}", t);
        }
    }

    timestamps.len()
}

/// Copies text to the system clipboard via standard Linux/macOS clipboard commands.
pub fn copy_to_clipboard(text: &str) -> bool {
    // 1. Try wl-copy (Wayland)
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }

    // 2. Try xclip (X11)
    if let Ok(mut child) = std::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }

    // 3. Try xsel (X11)
    if let Ok(mut child) = std::process::Command::new("xsel")
        .args(["--clipboard", "--input"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }

    // 4. Try pbcopy (macOS)
    if let Ok(mut child) = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_record_crash_and_check_loop() {
        let dir = tempdir().unwrap();
        let history = dir.path().join("crash_history");

        assert_eq!(record_crash_and_check_loop(1000, &history), 1);
        assert_eq!(record_crash_and_check_loop(1005, &history), 2);
        assert_eq!(record_crash_and_check_loop(1010, &history), 3);

        // Crash after 40 seconds should expire earlier timestamps
        assert_eq!(record_crash_and_check_loop(1055, &history), 1);
    }

    #[test]
    fn test_crash_log_read_and_clear() {
        let dir = tempdir().unwrap();
        let log_file = dir.path().join("test_crash.log");
        std::env::set_var("DOCSEARCH_CRASH_LOG", log_file.to_str().unwrap());

        assert_eq!(get_latest_crash_log(), None);

        fs::write(&log_file, "sample crash log content").unwrap();
        assert_eq!(get_latest_crash_log().as_deref(), Some("sample crash log content"));

        clear_crash_log();
        assert_eq!(get_latest_crash_log(), None);

        std::env::remove_var("DOCSEARCH_CRASH_LOG");
    }
}
