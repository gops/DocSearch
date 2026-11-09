slint::include_modules!();

mod cli;
mod config;
mod crawler;
mod extractor;
mod indexer;
mod tray;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use crawler::Crawler;
use extractor::ExtractorManager;
use indexer::duplicates::format_byte_size;
use indexer::Indexer;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::path::Path;
use std::sync::{Arc, RwLock};

const VERSION: &str = "0.2.3";
const BANNER: &str = r#"
  ___             ____                      _     
 |   \  ___  __  / ___|  ___  __ _ _ _ __ _| |_   
 | |) |/ _ \/ _| \___ \ / _ \/ _` | '_/ _| ' \  
 |___/ \___/\__| |____/ \___/\__,_|_| \__|_||_| 
  Local Cross-Platform Document Search Engine (Rust + Slint Native GUI)
"#;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    // Check if CLI subcommand was provided
    if let Some(cmd) = cli.command {
        let cfg = Config::load(cli.config.as_deref())?;
        match cmd {
            Commands::Search { query, limit } => {
                let idx = Indexer::open(&cfg.index_path)?;
                let res = idx.search(&query, limit)?;
                println!("\nFound {} matches for \"{}\" in {} ms:\n", res.total_hits, query, res.duration_ms);
                for (i, hit) in res.items.iter().enumerate() {
                    println!("[{}] {} ({})", i + 1, hit.filename, hit.path);
                    for h in &hit.highlights {
                        println!("    -> {}", h);
                    }
                    println!();
                }
            }
            Commands::Add { folder } => {
                let mut cfg = cfg;
                if cfg.add_folder(&folder) {
                    cfg.save()?;
                    println!("Successfully added folder: {}", folder);
                } else {
                    println!("Folder already in index list: {}", folder);
                }
            }
            Commands::List => {
                println!("\nConfigured Indexed Folders ({}):", cfg.indexed_folders.len());
                for (i, f) in cfg.indexed_folders.iter().enumerate() {
                    println!("  [{}] {}", i + 1, f);
                }
                let max_str = if cfg.max_file_size_mb == 0 {
                    "Unlimited".to_string()
                } else {
                    format!("{} MB", cfg.max_file_size_mb)
                };
                println!("\nMax File Size Limit: {} (larger files skipped)", max_str);
                println!("Indexing Pacing Delay: {} ms / file (gentle low I/O & CPU mode)", cfg.indexing_delay_ms);

                if let Ok(idx) = Indexer::open(&cfg.index_path) {
                    println!("Total Indexed Documents: {}\n", idx.doc_count());
                }
            }
            Commands::Reindex => {
                let idx = Arc::new(Indexer::open(&cfg.index_path)?);
                let ext = Arc::new(ExtractorManager::new(cfg.ocr_enabled, cfg.ocr_command.clone()));
                let config_arc = Arc::new(RwLock::new(cfg));
                let crawler = Crawler::new(config_arc, idx.clone(), ext);
                println!("Starting re-indexing...");
                crawler.scan_all();
                println!("Re-indexing complete. Current total documents: {}", idx.doc_count());
            }
            Commands::Duplicates => {
                let idx = Indexer::open(&cfg.index_path)?;
                let report = idx.find_duplicates()?;

                if report.total_groups == 0 {
                    println!("\n✅ No duplicate files found across your indexed folders.");
                    return Ok(());
                }

                println!(
                    "\n📑 Found {} duplicate groups ({} redundant copies, {} wasted disk space):\n",
                    report.total_groups,
                    report.total_duplicates,
                    format_byte_size(report.total_wasted_bytes)
                );

                for (i, grp) in report.groups.iter().enumerate() {
                    let hash_short = if grp.hash.len() > 12 { &grp.hash[..12] } else { &grp.hash };
                    println!(
                        "[Group {}] SHA-256: {}... | Size: {} | Wasted: {} ({} copies)",
                        i + 1,
                        hash_short,
                        format_byte_size(grp.size),
                        format_byte_size(grp.wasted_bytes),
                        grp.count
                    );
                    for (j, f) in grp.files.iter().enumerate() {
                        println!("   [{}] {}", j + 1, f.path);
                    }
                    println!();
                }
            }
        }
        return Ok(());
    }

    // Daemon & GUI mode
    print!("{}", BANNER);
    eprintln!("[Init] Starting DocSearch v{} (Rust + Slint)...", VERSION);

    let config = Config::load(cli.config.as_deref())?;
    let config_arc = Arc::new(RwLock::new(config.clone()));

    let indexer = Arc::new(Indexer::open(&config.index_path)?);
    let extractor = Arc::new(ExtractorManager::new(config.ocr_enabled, config.ocr_command.clone()));

    let available_pdf_tools = extractor::pdf::get_available_pdf_tools();
    if !available_pdf_tools.is_empty() {
        eprintln!(
            "[Init] PDF text extractor tool(s) detected: {}.",
            available_pdf_tools.join(", ")
        );
    } else {
        eprintln!(
            "[Notice] No PDF text extractor found (pdftotext / poppler-utils or gs / ghostscript).\n\
             To enable indexing of PDF documents, install either tool:\n\
               Ubuntu/Debian: sudo apt install poppler-utils  (or ghostscript)\n\
               Fedora/RHEL:   sudo dnf install poppler-utils  (or ghostscript)\n\
               Arch Linux:    sudo pacman -S poppler\n\
               macOS:         brew install poppler (or ghostscript)\n\
             (PDF files will be skipped until installed; DocSearch will not force install it.)"
        );
    }

    if extractor::ocr::is_ocr_available(&config.ocr_command) {
        eprintln!("[Init] OCR engine \"{}\" detected and active.", config.ocr_command);
    } else {
        eprintln!(
            "[Init] OCR engine \"{}\" not detected in PATH. Image/Scanned PDF OCR will be skipped.",
            config.ocr_command
        );
    }

    let crawler = Arc::new(Crawler::new(config_arc.clone(), indexer.clone(), extractor));

    // Start background live watcher
    crawler.start_watcher();

    // Trigger initial background directory scan
    let crawler_clone = crawler.clone();
    std::thread::spawn(move || {
        crawler_clone.scan_all();
    });

    // Setup system tray
    let (tray_tx, tray_rx) = crossbeam_channel::unbounded();
    tray::spawn_tray(tray_tx.clone());

    if cli.headless {
        eprintln!("[Init] Running in headless daemon mode. Press Ctrl+C to stop.");
        while let Ok(action) = tray_rx.recv() {
            match action {
                tray::TrayAction::Rescan => {
                    let c = crawler.clone();
                    std::thread::spawn(move || c.scan_all());
                }
                tray::TrayAction::Quit => {
                    eprintln!("[Shutdown] Shutting down DocSearch daemon...");
                    std::process::exit(0);
                }
                _ => {}
            }
        }
        return Ok(());
    }

    // Launch Slint GUI Application Window
    let main_window = AppWindow::new()?;
    let window_weak = main_window.as_weak();

    // Initialize UI properties
    update_folders_model(&main_window, &config_arc);
    main_window.set_has_pdf_tool(extractor::pdf::is_pdf_extractor_available());

    // Search query changed callback
    let idx_search = indexer.clone();
    let win_search = window_weak.clone();
    main_window.on_search_query_changed(move |query_str| {
        let trimmed = query_str.trim();
        if trimmed.is_empty() {
            if let Some(win) = win_search.upgrade() {
                win.set_results(ModelRc::new(VecModel::default()));
                win.set_has_results(false);
                win.set_metrics_text("".into());
            }
            return;
        }

        if let Ok(res) = idx_search.search(trimmed, 50) {
            if let Some(win) = win_search.upgrade() {
                let items: Vec<ResultItem> = res
                    .items
                    .into_iter()
                    .map(|item| {
                        let size_str = format_byte_size(item.size);
                        let mod_time_str = if item.mod_time > 0 {
                            let dt = chrono::DateTime::from_timestamp(item.mod_time, 0)
                                .map(|d| d.format("%Y-%m-%d").to_string())
                                .unwrap_or_default();
                            dt
                        } else {
                            String::new()
                        };

                        let highlight = item.highlights.join(" ... ");
                        let ext_clean = if item.extension.starts_with('.') {
                            item.extension[1..].to_uppercase()
                        } else {
                            item.extension.to_uppercase()
                        };

                        ResultItem {
                            path: item.path.into(),
                            filename: item.filename.into(),
                            extension: ext_clean.into(),
                            size_str: size_str.into(),
                            mod_time_str: mod_time_str.into(),
                            is_ocr: item.is_ocr,
                            highlight: highlight.into(),
                        }
                    })
                    .collect();

                let count = items.len();
                win.set_has_results(count > 0);
                win.set_results(ModelRc::new(VecModel::from(items)));
                win.set_metrics_text(format!("Found {} matches in {} ms", count, res.duration_ms).into());
            }
        }
    });

    // Native OS Folder Picker callback (NO text input!)
    let cfg_add = config_arc.clone();
    let crawler_add = crawler.clone();
    let win_add = window_weak.clone();
    main_window.on_add_folder_clicked(move || {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            let path_str = folder.to_string_lossy().to_string();
            let mut cfg = cfg_add.write().unwrap();
            if cfg.add_folder(&path_str) {
                let _ = cfg.save();
                drop(cfg);

                crawler_add.update_watches();
                let c = crawler_add.clone();
                std::thread::spawn(move || c.scan_all());

                if let Some(win) = win_add.upgrade() {
                    update_folders_model(&win, &cfg_add);
                }
            }
        }
    });

    // Remove folder callback
    let cfg_remove = config_arc.clone();
    let win_remove = window_weak.clone();
    main_window.on_remove_folder(move |folder_str| {
        let mut cfg = cfg_remove.write().unwrap();
        if cfg.remove_folder(&folder_str) {
            let _ = cfg.save();
            drop(cfg);
            if let Some(win) = win_remove.upgrade() {
                update_folders_model(&win, &cfg_remove);
            }
        }
    });

    // Trigger re-scan callback
    let crawler_rescan = crawler.clone();
    main_window.on_trigger_rescan(move || {
        let c = crawler_rescan.clone();
        std::thread::spawn(move || c.scan_all());
    });

    // Duplicates callback
    let idx_dupes = indexer.clone();
    let win_dupes = window_weak.clone();
    main_window.on_refresh_duplicates(move || {
        if let Ok(report) = idx_dupes.find_duplicates() {
            if let Some(win) = win_dupes.upgrade() {
                let summary = format!(
                    "{} duplicate groups ({} wasted space)",
                    report.total_groups,
                    format_byte_size(report.total_wasted_bytes)
                );
                win.set_duplicates_summary(summary.into());

                let groups: Vec<DuplicateGroupItem> = report
                    .groups
                    .into_iter()
                    .map(|grp| {
                        let hash_short = if grp.hash.len() > 10 {
                            &grp.hash[..10]
                        } else {
                            &grp.hash
                        };
                        let first = grp.files.get(0).map(|f| f.path.clone()).unwrap_or_default();
                        let second = grp.files.get(1).map(|f| f.path.clone()).unwrap_or_default();

                        DuplicateGroupItem {
                            hash_short: hash_short.into(),
                            size_str: format_byte_size(grp.size).into(),
                            wasted_str: format_byte_size(grp.wasted_bytes).into(),
                            count: grp.count as i32,
                            first_path: first.into(),
                            second_path: second.into(),
                        }
                    })
                    .collect();

                win.set_duplicate_groups(ModelRc::new(VecModel::from(groups)));
            }
        }
    });

    // Open file action
    main_window.on_open_file(move |path_str| {
        let p = Path::new(path_str.as_str());
        let _ = open::that(p);
    });

    // Show in folder action
    main_window.on_show_in_folder(move |path_str| {
        let p = Path::new(path_str.as_str());
        if let Some(parent) = p.parent() {
            let _ = open::that(parent);
        } else {
            let _ = open::that(p);
        }
    });

    // Delete duplicate file action
    let idx_del = indexer.clone();
    let win_del = window_weak.clone();
    main_window.on_delete_duplicate_file(move |path_str| {
        let path = path_str.to_string();
        if std::fs::remove_file(&path).is_ok() {
            let _ = idx_del.delete_document(&path);
            let _ = idx_del.commit();
            if let Some(win) = win_del.upgrade() {
                // Refresh duplicates
                if let Ok(report) = idx_del.find_duplicates() {
                    let summary = format!(
                        "{} duplicate groups ({} wasted space)",
                        report.total_groups,
                        format_byte_size(report.total_wasted_bytes)
                    );
                    win.set_duplicates_summary(summary.into());
                }
            }
        }
    });

    // Status Timer: Periodically check indexing status
    let crawler_timer = crawler.clone();
    let win_timer = window_weak.clone();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || {
        let status = crawler_timer.get_status();
        if let Some(win) = win_timer.upgrade() {
            win.set_is_indexing(status.is_indexing);
            if status.is_indexing {
                win.set_status_text(format!("Indexing ({}/{} files)...", status.indexed_count, status.total_files).into());
            } else {
                win.set_status_text("Idle".into());
            }
        }
    });

    // Handle window close request: minimize to system tray instead of closing
    main_window.window().on_close_requested(|| {
        eprintln!("[GUI] Window close requested: minimizing to system tray.");
        slint::CloseRequestResponse::HideWindow
    });

    // Tray Listener Thread: Handles tray clicks
    let win_tray = window_weak.clone();
    let crawler_tray = crawler.clone();
    std::thread::spawn(move || {
        while let Ok(action) = tray_rx.recv() {
            match action {
                tray::TrayAction::ShowWindow => {
                    let _ = slint::invoke_from_event_loop({
                        let win = win_tray.clone();
                        move || {
                            if let Some(w) = win.upgrade() {
                                let _ = w.show();
                                w.window().set_minimized(false);
                            }
                        }
                    });
                }
                tray::TrayAction::Rescan => {
                    let c = crawler_tray.clone();
                    std::thread::spawn(move || c.scan_all());
                }
                tray::TrayAction::Quit => {
                    eprintln!("[Shutdown] Quit requested from system tray. Terminating DocSearch...");
                    std::process::exit(0);
                }
            }
        }
    });

    // Show window initially and run event loop until explicit quit from system tray
    main_window.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}

fn update_folders_model(win: &AppWindow, config: &Arc<RwLock<Config>>) {
    let folders: Vec<FolderItem> = {
        let cfg = config.read().unwrap();
        cfg.indexed_folders
            .iter()
            .map(|f| FolderItem { path: f.clone().into() })
            .collect()
    };
    win.set_folder_count(folders.len() as i32);
    win.set_folders(ModelRc::new(VecModel::from(folders)));
}
