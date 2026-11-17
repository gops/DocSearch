use docsearch::autostart;
use tempfile::tempdir;

#[test]
fn test_autostart_enable_disable_lifecycle() {
    let tmp = tempdir().unwrap();
    let autostart_file = tmp.path().join("autostart").join("docsearch.desktop");
    std::env::set_var("DOCSEARCH_AUTOSTART_PATH", &autostart_file);

    // Initial state: not enabled
    assert!(!autostart::is_autostart_enabled());

    // 1. Enable autostart
    autostart::enable_autostart().expect("Failed to enable autostart");
    assert!(autostart_file.exists());
    assert!(autostart::is_autostart_enabled());

    // Verify desktop file contents
    let content = std::fs::read_to_string(&autostart_file).unwrap();
    assert!(content.contains("[Desktop Entry]"));
    assert!(content.contains("Name=DocSearch"));
    assert!(content.contains("--minimized"));
    assert!(content.contains("X-GNOME-Autostart-enabled=true"));

    // 2. Disable autostart
    autostart::disable_autostart().expect("Failed to disable autostart");
    assert!(!autostart_file.exists());
    assert!(!autostart::is_autostart_enabled());

    // 3. Toggle autostart (off -> on -> off)
    let state1 = autostart::toggle_autostart().expect("Failed to toggle autostart to on");
    assert!(state1);
    assert!(autostart::is_autostart_enabled());
    assert!(autostart_file.exists());

    let state2 = autostart::toggle_autostart().expect("Failed to toggle autostart to off");
    assert!(!state2);
    assert!(!autostart::is_autostart_enabled());
    assert!(!autostart_file.exists());

    std::env::remove_var("DOCSEARCH_AUTOSTART_PATH");
}
