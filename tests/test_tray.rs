use crossbeam_channel::unbounded;
use docsearch::tray::{DocSearchTray, StartupBinaryInfo, TrayAction};
use ksni::{MenuItem, Tray};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_tray_menu_conditional_restart_option() {
    let dir = tempdir().unwrap();
    let dummy_exe = dir.path().join("docsearch_app");

    // Write initial binary
    {
        let mut f = File::create(&dummy_exe).unwrap();
        f.write_all(b"initial_binary_bytes_v1").unwrap();
        f.sync_all().unwrap();
    }

    let meta = std::fs::metadata(&dummy_exe).unwrap();
    let orig_mtime = meta.modified().ok();
    let orig_size = Some(meta.len());
    #[cfg(unix)]
    let orig_ino = {
        use std::os::unix::fs::MetadataExt;
        Some(meta.ino())
    };

    let startup_info = StartupBinaryInfo {
        clean_path: dummy_exe.clone(),
        orig_mtime,
        #[cfg(unix)]
        orig_ino,
        orig_size,
    };

    let (tx, _rx) = unbounded::<TrayAction>();
    let tray = DocSearchTray {
        sender: tx,
        startup_info,
    };

    // 1. Initially: Binary is unchanged. The menu should NOT have "Restart to Apply Update".
    let menu = tray.menu();
    let labels: Vec<String> = menu
        .iter()
        .filter_map(|item| match item {
            MenuItem::Standard(s) => Some(s.label.clone()),
            _ => None,
        })
        .collect();

    assert!(!labels.iter().any(|l| l.contains("Restart to Apply Update")));
    assert!(labels.iter().any(|l| l.contains("Open DocSearch")));
    assert!(labels.iter().any(|l| l.contains("Re-scan Folders")));
    assert!(labels.iter().any(|l| l.contains("Quit")));

    // 2. Simulate package manager / cargo replacement (different file at same path)
    std::thread::sleep(std::time::Duration::from_millis(50));
    let new_temp = dir.path().join("docsearch_app.tmp");
    {
        let mut f = File::create(&new_temp).unwrap();
        f.write_all(b"updated_binary_bytes_v2_with_new_features").unwrap();
        f.sync_all().unwrap();
    }
    std::fs::rename(&new_temp, &dummy_exe).unwrap();

    // 3. Now: Binary on disk has been replaced! The menu MUST now include "Restart to Apply Update".
    let updated_menu = tray.menu();
    let updated_labels: Vec<String> = updated_menu
        .iter()
        .filter_map(|item| match item {
            MenuItem::Standard(s) => Some(s.label.clone()),
            _ => None,
        })
        .collect();

    assert!(updated_labels.iter().any(|l| l.contains("Restart to Apply Update")));
    // The restart option should be the first item
    assert_eq!(updated_labels[0], "🚀 Restart to Apply Update");
}
