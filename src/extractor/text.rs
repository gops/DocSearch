use std::fs;
use std::io::Read;
use std::path::Path;

pub fn extract_plain_text<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    let file = fs::File::open(path)?;
    // Read up to 2MB text
    let mut buffer = Vec::new();
    file.take(2 * 1024 * 1024).read_to_end(&mut buffer)?;

    // Lossy UTF-8 conversion to avoid panic on non-utf8 characters in code/logs
    let text = String::from_utf8_lossy(&buffer).to_string();
    Ok(text)
}
