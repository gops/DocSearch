use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub indexed_folders: Vec<String>,
    #[serde(default = "default_index_path")]
    pub index_path: String,
    #[serde(default = "default_server_port")]
    pub server_port: Option<u16>,
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_true")]
    pub ocr_enabled: bool,
    #[serde(default = "default_ocr_command")]
    pub ocr_command: String,
    #[serde(default = "default_github_repo")]
    pub github_repo: String,
    #[serde(default = "default_file_extensions")]
    pub file_extensions: Vec<String>,
    #[serde(default = "default_excluded_dirs")]
    pub excluded_dirs: Vec<String>,

    #[serde(skip)]
    pub config_file_path: PathBuf,
}

fn default_index_path() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home).join(".local/share/docsearch/tantivy_index");
        path.to_string_lossy().to_string()
    } else {
        "data/tantivy_index".to_string()
    }
}

fn default_server_port() -> Option<u16> {
    Some(9876)
}

fn default_max_file_size_mb() -> u64 {
    25
}

fn default_true() -> bool {
    true
}

fn default_ocr_command() -> String {
    "tesseract".to_string()
}

fn default_github_repo() -> String {
    "gops/DocSearch".to_string()
}

fn default_file_extensions() -> Vec<String> {
    vec![
        ".txt".into(), ".md".into(), ".markdown".into(), ".pdf".into(), ".docx".into(),
        ".csv".into(), ".log".into(), ".json".into(), ".yaml".into(), ".yml".into(),
        ".png".into(), ".jpg".into(), ".jpeg".into(), ".rs".into(), ".go".into(),
        ".py".into(), ".js".into(), ".ts".into(), ".html".into(), ".css".into(), ".sh".into(),
    ]
}

fn default_excluded_dirs() -> Vec<String> {
    vec![
        ".git".into(), "node_modules".into(), ".cache".into(),
        "vendor".into(), "tmp".into(), "target".into(),
    ]
}

impl Default for Config {
    fn default() -> Self {
        let config_path = Self::default_config_path();
        Self {
            indexed_folders: Vec::new(),
            index_path: default_index_path(),
            server_port: default_server_port(),
            max_file_size_mb: default_max_file_size_mb(),
            ocr_enabled: default_true(),
            ocr_command: default_ocr_command(),
            github_repo: default_github_repo(),
            file_extensions: default_file_extensions(),
            excluded_dirs: default_excluded_dirs(),
            config_file_path: config_path,
        }
    }
}

impl Config {
    pub fn default_config_path() -> PathBuf {
        if let Ok(custom) = std::env::var("DOCSEARCH_CONFIG") {
            return PathBuf::from(custom);
        }

        if Path::new("data/config.json").exists() {
            return PathBuf::from("data/config.json");
        }

        if let Ok(home) = std::env::var("HOME") {
            let config_dir = PathBuf::from(home).join(".config/docsearch");
            let _ = fs::create_dir_all(&config_dir);
            return config_dir.join("config.json");
        }

        PathBuf::from("data/config.json")
    }

    pub fn load(custom_path: Option<&str>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = match custom_path {
            Some(p) => PathBuf::from(p),
            None => Self::default_config_path(),
        };

        if !path.exists() {
            let mut cfg = Config::default();
            cfg.config_file_path = path;
            let _ = cfg.save();
            return Ok(cfg);
        }

        let content = fs::read_to_string(&path)?;
        let mut cfg: Config = serde_json::from_str(&content)?;
        cfg.config_file_path = path;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = self.config_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&self.config_file_path, json)?;
        Ok(())
    }

    pub fn add_folder(&mut self, folder: &str) -> bool {
        let canonical = match fs::canonicalize(folder) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => folder.to_string(),
        };

        if !self.indexed_folders.contains(&canonical) {
            self.indexed_folders.push(canonical);
            true
        } else {
            false
        }
    }

    pub fn remove_folder(&mut self, folder: &str) -> bool {
        let canonical = match fs::canonicalize(folder) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => folder.to_string(),
        };

        if let Some(pos) = self.indexed_folders.iter().position(|f| f == &canonical || f == folder) {
            self.indexed_folders.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn is_allowed_extension(&self, ext: &str) -> bool {
        let lower = ext.to_lowercase();
        self.file_extensions.iter().any(|e| e.to_lowercase() == lower)
    }

    pub fn is_excluded_dir(&self, dir_name: &str) -> bool {
        self.excluded_dirs.iter().any(|d| d == dir_name)
    }

    pub fn is_file_too_large(&self, size_bytes: u64) -> bool {
        if self.max_file_size_mb == 0 {
            return false;
        }
        size_bytes > self.max_file_size_mb * 1024 * 1024
    }
}
