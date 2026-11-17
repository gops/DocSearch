use docsearch::config::Config;
use tempfile::TempDir;

#[test]
fn test_config_defaults_and_serialization() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("config.json");

    let mut cfg = Config::default();
    cfg.config_file_path = config_file.clone();

    assert!(cfg.add_folder("/tmp/sample_folder"));
    // Adding again returns false
    assert!(!cfg.add_folder("/tmp/sample_folder"));

    assert!(cfg.is_allowed_extension(".pdf"));
    assert!(cfg.is_allowed_extension(".docx"));
    assert!(!cfg.is_allowed_extension(".exe"));

    assert!(cfg.is_excluded_dir(".git"));
    assert!(cfg.is_excluded_dir("node_modules"));
    assert!(!cfg.is_excluded_dir("my_notes"));

    assert!(cfg.is_file_too_large(30 * 1024 * 1024)); // 30MB > 25MB
    assert!(!cfg.is_file_too_large(10 * 1024 * 1024)); // 10MB < 25MB

    cfg.save().expect("Failed to save config");

    let loaded = Config::load(Some(config_file.to_str().unwrap())).expect("Failed to load config");
    assert_eq!(loaded.indexed_folders.len(), 1);
    assert_eq!(loaded.max_file_size_mb, 25);
    assert_eq!(loaded.indexing_delay_ms, 25);
    assert_eq!(loaded.theme, "auto");
    assert_eq!(loaded.autostart, false);
}

#[test]
fn test_theme_day_night_and_cycling() {
    let mut cfg = Config::default();
    assert_eq!(cfg.theme, "auto");

    // In auto mode, theme resolves based on daytime (6..18)
    let is_day = Config::is_daytime();
    assert_eq!(cfg.resolved_is_dark(), !is_day);

    let (is_dark, display) = cfg.get_theme_display();
    assert_eq!(is_dark, !is_day);
    if is_day {
        assert!(display.contains("Light (Auto)"));
    } else {
        assert!(display.contains("Dark (Auto)"));
    }

    // Force Dark
    cfg.theme = "dark".to_string();
    assert_eq!(cfg.resolved_is_dark(), true);
    assert_eq!(cfg.get_theme_display(), (true, "🌙 Dark".to_string()));

    // Force Light
    cfg.theme = "light".to_string();
    assert_eq!(cfg.resolved_is_dark(), false);
    assert_eq!(cfg.get_theme_display(), (false, "☀️ Light".to_string()));

    // Cycling
    cfg.theme = "auto".to_string();
    let next1 = cfg.cycle_theme();
    let next2 = cfg.cycle_theme();
    let next3 = cfg.cycle_theme();
    assert_eq!(next3, "auto");
    assert!(next1 == "dark" || next1 == "light");
    assert!(next2 == "dark" || next2 == "light");
    assert_ne!(next1, next2);
}
