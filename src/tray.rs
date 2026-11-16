use crossbeam_channel::Sender;
use ksni::{blocking::TrayMethods, menu::StandardItem, MenuItem, Tray};
use std::path::PathBuf;
use std::time::SystemTime;

pub enum TrayAction {
    ShowWindow,
    Rescan,
    Restart,
    Quit,
}

#[derive(Clone, Debug)]
pub struct StartupBinaryInfo {
    pub clean_path: PathBuf,
    pub orig_mtime: Option<SystemTime>,
    #[cfg(unix)]
    pub orig_ino: Option<u64>,
    pub orig_size: Option<u64>,
}

impl StartupBinaryInfo {
    pub fn record() -> Self {
        let clean_path = resolve_clean_exe_path();
        let meta = std::fs::metadata(&clean_path).ok();
        let orig_mtime = meta.as_ref().and_then(|m| m.modified().ok());
        let orig_size = meta.as_ref().map(|m| m.len());
        #[cfg(unix)]
        let orig_ino = {
            use std::os::unix::fs::MetadataExt;
            meta.as_ref().map(|m| m.ino())
        };

        Self {
            clean_path,
            orig_mtime,
            #[cfg(unix)]
            orig_ino,
            orig_size,
        }
    }

    pub fn is_updated(&self) -> bool {
        // Environment override for testing / simulation
        if std::env::var("DOCSEARCH_FORCE_UPDATE_AVAILABLE").is_ok() {
            return true;
        }

        // 1. Check if /proc/self/exe has been marked as deleted (Linux dpkg/apt upgrade behavior)
        #[cfg(target_os = "linux")]
        {
            if let Ok(link) = std::fs::read_link("/proc/self/exe") {
                let s = link.to_string_lossy();
                if s.ends_with(" (deleted)") && self.clean_path.is_file() {
                    return true;
                }
            }
        }

        // 2. Check metadata at clean_path
        let meta = match std::fs::metadata(&self.clean_path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        if !meta.is_file() {
            return false;
        }

        // 3. On Unix, check if inode changed (e.g. replaced by atomic rename/install)
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Some(orig_ino) = self.orig_ino {
                if meta.ino() != orig_ino {
                    return true;
                }
            }
        }

        // 4. Check if mtime changed / is newer
        if let (Some(orig_mtime), Ok(curr_mtime)) = (self.orig_mtime, meta.modified()) {
            if curr_mtime > orig_mtime {
                return true;
            }
        }

        // 5. Check if size changed
        if let Some(orig_size) = self.orig_size {
            if meta.len() != orig_size {
                return true;
            }
        }

        false
    }
}

pub fn resolve_clean_exe_path() -> PathBuf {
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("docsearch"));
    let path_str = current_exe.to_string_lossy().to_string();
    let clean_str = if path_str.ends_with(" (deleted)") {
        path_str[..path_str.len() - " (deleted)".len()].to_string()
    } else {
        path_str
    };
    PathBuf::from(clean_str)
}

pub fn restart_application() {
    let clean_path = resolve_clean_exe_path();
    let args: Vec<String> = std::env::args().skip(1).collect();

    eprintln!("[Restart] Relaunching DocSearch: {:?} with args {:?}", clean_path, args);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&clean_path).args(&args).exec();
        eprintln!("[Restart] exec failed: {}, falling back to spawn...", err);
    }

    match std::process::Command::new(&clean_path).args(&args).spawn() {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[Restart] Failed to spawn updated DocSearch process: {}", e);
        }
    }
}

pub struct DocSearchTray {
    pub sender: Sender<TrayAction>,
    pub startup_info: StartupBinaryInfo,
}

impl Tray for DocSearchTray {
    fn id(&self) -> String {
        "docsearch".into()
    }

    fn title(&self) -> String {
        "DocSearch".into()
    }

    fn icon_name(&self) -> String {
        "system-search".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::ShowWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = Vec::new();

        // Dynamically show restart option when binary has been updated on disk
        if self.startup_info.is_updated() {
            let tx_restart = self.sender.clone();
            items.push(
                StandardItem {
                    label: "🚀 Restart to Apply Update".into(),
                    activate: Box::new(move |_| {
                        let _ = tx_restart.send(TrayAction::Restart);
                    }),
                    ..Default::default()
                }
                .into(),
            );
            items.push(MenuItem::Separator);
        }

        let tx_open = self.sender.clone();
        let tx_rescan = self.sender.clone();
        let tx_quit = self.sender.clone();

        items.push(
            StandardItem {
                label: "🔍 Open DocSearch".into(),
                activate: Box::new(move |_| {
                    let _ = tx_open.send(TrayAction::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "🔄 Re-scan Folders".into(),
                activate: Box::new(move |_| {
                    let _ = tx_rescan.send(TrayAction::Rescan);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "❌ Quit".into(),
                activate: Box::new(move |_| {
                    let _ = tx_quit.send(TrayAction::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

#[allow(dead_code)]
pub fn spawn_tray(sender: Sender<TrayAction>) {
    let startup_info = StartupBinaryInfo::record();
    spawn_tray_with_info(sender, startup_info);
}

pub fn spawn_tray_with_info(sender: Sender<TrayAction>, startup_info: StartupBinaryInfo) {
    let tray = DocSearchTray {
        sender,
        startup_info,
    };
    let _ = tray.disable_dbus_name(true).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_startup_binary_info_detects_update_on_inode_or_size() {
        let dir = tempdir().unwrap();
        let bin_path = dir.path().join("dummy_docsearch");

        // Create initial dummy binary
        {
            let mut f = File::create(&bin_path).unwrap();
            f.write_all(b"version 1 binary content").unwrap();
            f.sync_all().unwrap();
        }

        let meta = std::fs::metadata(&bin_path).unwrap();
        let orig_mtime = meta.modified().ok();
        let orig_size = Some(meta.len());
        #[cfg(unix)]
        let orig_ino = {
            use std::os::unix::fs::MetadataExt;
            Some(meta.ino())
        };

        let info = StartupBinaryInfo {
            clean_path: bin_path.clone(),
            orig_mtime,
            #[cfg(unix)]
            orig_ino,
            orig_size,
        };

        // Initially not updated
        assert!(!info.is_updated());

        // Replace binary with new file / content (simulating dpkg or cargo replace)
        std::thread::sleep(std::time::Duration::from_millis(50));
        let new_temp = dir.path().join("dummy_docsearch.tmp");
        {
            let mut f = File::create(&new_temp).unwrap();
            f.write_all(b"version 2 updated binary content with new features").unwrap();
            f.sync_all().unwrap();
        }
        std::fs::rename(&new_temp, &bin_path).unwrap();

        // Now it must detect the update
        assert!(info.is_updated());
    }

    #[test]
    fn test_resolve_clean_exe_path() {
        let path = resolve_clean_exe_path();
        assert!(!path.to_string_lossy().ends_with(" (deleted)"));
    }
}
