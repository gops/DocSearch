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
}
