use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateFile {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub mod_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: i64,
    pub wasted_bytes: i64,
    pub count: usize,
    pub files: Vec<DuplicateFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateReport {
    pub total_groups: usize,
    pub total_duplicates: usize,
    pub total_wasted_bytes: i64,
    pub groups: Vec<DuplicateGroup>,
}

pub fn format_byte_size(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 B".to_string();
    }
    const SIZES: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes as f64;
    let mut i = 0;
    while val >= 1024.0 && i < SIZES.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", val, SIZES[i])
}
