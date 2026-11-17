use std::path::PathBuf;

/// Resolves the file path where the autostart .desktop entry is saved.
pub fn get_autostart_desktop_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(custom) = std::env::var("DOCSEARCH_AUTOSTART_PATH") {
        return Ok(PathBuf::from(custom));
    }

    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(xdg).join("autostart").join("docsearch.desktop"))
    } else if let Ok(home) = std::env::var("HOME") {
        Ok(PathBuf::from(home).join(".config").join("autostart").join("docsearch.desktop"))
    } else {
        Err("Could not determine user configuration or home directory for autostart".into())
    }
}

/// Checks whether DocSearch is currently configured to automatically launch on system startup.
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(path) = get_autostart_desktop_path() {
            if path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("X-GNOME-Autostart-enabled=false") {
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Enables DocSearch to automatically launch on system startup (minimized to system tray).
pub fn enable_autostart() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "linux")]
    {
        let path = get_autostart_desktop_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let exe_path = crate::tray::resolve_clean_exe_path();
        let exe_str = exe_path.to_string_lossy();
        let exec_cmd = if exe_str.contains(' ') {
            format!("\"{}\" --minimized", exe_str)
        } else {
            format!("{} --minimized", exe_str)
        };

        let desktop_content = format!(
            "[Desktop Entry]\n\
            Type=Application\n\
            Version=1.0\n\
            Name=DocSearch\n\
            GenericName=Document Search Engine\n\
            Comment=Fast local document indexing & search with native Slint GUI\n\
            Exec={}\n\
            Icon=docsearch\n\
            Terminal=false\n\
            Categories=Utility;Filesystem;Office;\n\
            StartupNotify=false\n\
            X-GNOME-Autostart-enabled=true\n",
            exec_cmd
        );

        std::fs::write(&path, desktop_content)?;
        eprintln!("[Autostart] Registered autostart desktop entry at {:?}", path);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Autostart registration is currently supported on Linux desktops".into())
    }
}

/// Disables automatic startup by removing the autostart desktop entry.
pub fn disable_autostart() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "linux")]
    {
        let path = get_autostart_desktop_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
            eprintln!("[Autostart] Removed autostart desktop entry at {:?}", path);
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("Autostart registration is currently supported on Linux desktops".into())
    }
}

/// Toggles the current autostart state and returns the new state (true = enabled, false = disabled).
pub fn toggle_autostart() -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if is_autostart_enabled() {
        disable_autostart()?;
        Ok(false)
    } else {
        enable_autostart()?;
        Ok(true)
    }
}
