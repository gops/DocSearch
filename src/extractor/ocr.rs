use std::path::Path;
use std::process::Command;

pub fn is_ocr_available(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn extract_image_ocr<P: AsRef<Path>>(
    image_path: P,
    ocr_cmd: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new(ocr_cmd)
        .arg(image_path.as_ref())
        .arg("stdout")
        .arg("--oem")
        .arg("1")
        .output();

    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => {
            // Retry without extra args
            Command::new(ocr_cmd)
                .arg(image_path.as_ref())
                .arg("stdout")
                .output()?
        }
    };

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("OCR tool exited with error: {}", err_msg).into());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("OCR produced empty output".into());
    }

    Ok(text)
}
