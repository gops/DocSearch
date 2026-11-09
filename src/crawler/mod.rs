use crate::config::Config;
use crate::extractor::ExtractorManager;
use crate::indexer::Indexer;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, UNIX_EPOCH};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct CrawlerStatus {
    pub is_indexing: bool,
    pub indexed_count: usize,
    pub total_files: usize,
    #[allow(dead_code)]
    pub last_index_time: String,
}

pub struct Crawler {
    config: Arc<RwLock<Config>>,
    indexer: Arc<Indexer>,
    extractor: Arc<ExtractorManager>,

    pub is_indexing: Arc<AtomicBool>,
    pub indexed_count: Arc<AtomicUsize>,
    pub total_files: Arc<AtomicUsize>,
    pub last_index_time: Arc<RwLock<String>>,

    watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
}

impl Crawler {
    pub fn new(
        config: Arc<RwLock<Config>>,
        indexer: Arc<Indexer>,
        extractor: Arc<ExtractorManager>,
    ) -> Self {
        Self {
            config,
            indexer,
            extractor,
            is_indexing: Arc::new(AtomicBool::new(false)),
            indexed_count: Arc::new(AtomicUsize::new(0)),
            total_files: Arc::new(AtomicUsize::new(0)),
            last_index_time: Arc::new(RwLock::new(String::new())),
            watcher: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_status(&self) -> CrawlerStatus {
        CrawlerStatus {
            is_indexing: self.is_indexing.load(Ordering::Relaxed),
            indexed_count: self.indexed_count.load(Ordering::Relaxed),
            total_files: self.total_files.load(Ordering::Relaxed),
            last_index_time: self.last_index_time.read().unwrap().clone(),
        }
    }

    pub fn scan_all(&self) {
        if self.is_indexing.swap(true, Ordering::SeqCst) {
            eprintln!("[Crawler] Scan already in progress, skipping...");
            return;
        }

        let folders = {
            let cfg = self.config.read().unwrap();
            cfg.indexed_folders.clone()
        };

        if folders.is_empty() {
            eprintln!("[Crawler] No folders configured to index yet.");
            self.is_indexing.store(false, Ordering::SeqCst);
            return;
        }

        eprintln!("[Crawler] Starting scan on {} configured folders...", folders.len());

        let mut candidate_files = Vec::new();
        {
            let cfg = self.config.read().unwrap();
            for folder in &folders {
                let folder_path = Path::new(folder);
                if !folder_path.exists() {
                    continue;
                }

                for entry in WalkDir::new(folder_path)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| {
                        if e.file_type().is_dir() {
                            let name = e.file_name().to_string_lossy();
                            !cfg.is_excluded_dir(&name)
                        } else {
                            true
                        }
                    })
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        let path = entry.path();
                        let ext = path
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map(|s| format!(".{}", s))
                            .unwrap_or_default();

                        if cfg.is_allowed_extension(&ext) {
                            if let Ok(meta) = entry.metadata() {
                                if !cfg.is_file_too_large(meta.len()) {
                                    candidate_files.push(path.to_path_buf());
                                }
                            }
                        }
                    }
                }
            }
        }

        self.total_files.store(candidate_files.len(), Ordering::SeqCst);
        self.indexed_count.store(0, Ordering::SeqCst);
        eprintln!(
            "[Crawler] Discovered {} candidate files. Indexing with gentle background pacing...",
            candidate_files.len()
        );

        let mut indexed_in_batch = 0usize;
        for path in &candidate_files {
            let was_indexed = self.index_single_file(path);
            self.indexed_count.fetch_add(1, Ordering::Relaxed);

            let delay_ms = {
                let cfg = self.config.read().unwrap();
                cfg.indexing_delay_ms
            };

            // Gentle sleep between files to prevent hard disk I/O and CPU spikes
            if delay_ms > 0 {
                let nap = if was_indexed {
                    Duration::from_millis(delay_ms.saturating_mul(2))
                } else {
                    Duration::from_millis(delay_ms)
                };
                std::thread::sleep(nap);
            }

            if was_indexed {
                indexed_in_batch += 1;
                // Periodic batch commit every 25 newly indexed files to avoid heavy end-of-scan commit spikes
                if indexed_in_batch >= 25 {
                    indexed_in_batch = 0;
                    let _ = self.indexer.commit();
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }

        let _ = self.indexer.commit();

        if let Ok(mut time_guard) = self.last_index_time.write() {
            *time_guard = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }

        self.is_indexing.store(false, Ordering::SeqCst);
        eprintln!("[Crawler] Scan completed. Total documents in index: {}", self.indexer.doc_count());
    }

    pub fn index_single_file(&self, path: &Path) -> bool {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };

        {
            let cfg = self.config.read().unwrap();
            if cfg.is_file_too_large(meta.len()) {
                return false;
            }
        }

        let hash = match calculate_file_hash(path) {
            Ok(h) => h,
            Err(_) => return false,
        };

        let path_str = path.to_string_lossy().to_string();

        // Check if unchanged
        if let Ok(Some(stored_hash)) = self.indexer.get_document_hash(&path_str) {
            if stored_hash == hash {
                return false; // Unchanged!
            }
        }

        // Extract content
        let extracted = match self.extractor.extract(path) {
            Ok(res) => res,
            Err(_) => return false,
        };

        if extracted.content.trim().is_empty() {
            return false;
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| format!(".{}", s)).unwrap_or_default();
        let mod_time = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let res = self.indexer.index_document(
            &path_str,
            &filename,
            &ext,
            meta.len() as i64,
            mod_time,
            &hash,
            extracted.is_ocr,
            &extracted.content,
        );

        res.is_ok()
    }

    pub fn start_watcher(&self) {
        let folders = {
            let cfg = self.config.read().unwrap();
            cfg.indexed_folders.clone()
        };

        let indexer = self.indexer.clone();
        let extractor = self.extractor.clone();
        let config = self.config.clone();

        let (tx, rx) = crossbeam_channel::unbounded();

        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[Crawler] Failed to initialize watcher: {}", e);
                return;
            }
        };

        for folder in &folders {
            let p = Path::new(folder);
            if p.exists() {
                let _ = watcher.watch(p, RecursiveMode::Recursive);
            }
        }

        if let Ok(mut w_guard) = self.watcher.lock() {
            *w_guard = Some(watcher);
        }

        // Event handler thread
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        let mut has_changes = false;
                        for path in event.paths {
                            if path.is_file() {
                                let ext = path
                                    .extension()
                                    .and_then(|s| s.to_str())
                                    .map(|s| format!(".{}", s))
                                    .unwrap_or_default();

                                let (allowed, delay_ms) = {
                                    let cfg = config.read().unwrap();
                                    (cfg.is_allowed_extension(&ext), cfg.indexing_delay_ms)
                                };

                                if allowed {
                                    let path_str = path.to_string_lossy().to_string();
                                    if let Ok(meta) = fs::metadata(&path) {
                                        {
                                            let cfg = config.read().unwrap();
                                            if cfg.is_file_too_large(meta.len()) {
                                                continue;
                                            }
                                        }

                                        if let Ok(hash) = calculate_file_hash(&path) {
                                            // Check if unchanged
                                            if let Ok(Some(stored_hash)) = indexer.get_document_hash(&path_str) {
                                                if stored_hash == hash {
                                                    continue;
                                                }
                                            }

                                            // Gentle pacing pause before extraction
                                            if delay_ms > 0 {
                                                std::thread::sleep(Duration::from_millis(delay_ms));
                                            }

                                            if let Ok(extracted) = extractor.extract(&path) {
                                                if !extracted.content.trim().is_empty() {
                                                    let filename = path
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                        .unwrap_or("")
                                                        .to_string();
                                                    let mod_time = meta
                                                        .modified()
                                                        .ok()
                                                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                                                        .map(|d| d.as_secs() as i64)
                                                        .unwrap_or(0);

                                                    let _ = indexer.index_document(
                                                        &path_str,
                                                        &filename,
                                                        &ext,
                                                        meta.len() as i64,
                                                        mod_time,
                                                        &hash,
                                                        extracted.is_ocr,
                                                        &extracted.content,
                                                    );
                                                    has_changes = true;

                                                    // Post-index pause to avoid CPU/disk saturation
                                                    if delay_ms > 0 {
                                                        std::thread::sleep(Duration::from_millis(delay_ms));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if has_changes {
                            let _ = indexer.commit();
                        }
                    }
                    EventKind::Remove(_) => {
                        let mut has_changes = false;
                        for path in event.paths {
                            let path_str = path.to_string_lossy().to_string();
                            let _ = indexer.delete_document(&path_str);
                            has_changes = true;
                        }
                        if has_changes {
                            let _ = indexer.commit();
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    pub fn update_watches(&self) {
        if let Ok(mut w_guard) = self.watcher.lock() {
            if let Some(ref mut watcher) = *w_guard {
                let folders = {
                    let cfg = self.config.read().unwrap();
                    cfg.indexed_folders.clone()
                };
                for folder in folders {
                    let p = Path::new(&folder);
                    if p.exists() {
                        let _ = watcher.watch(p, RecursiveMode::Recursive);
                    }
                }
            }
        }
    }
}

pub fn calculate_file_hash<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes_since_yield = 0usize;

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes_since_yield += count;
        // Yield to OS every 1MB so reading large files does not monopolize CPU or saturate disk bandwidth
        if bytes_since_yield >= 1024 * 1024 {
            bytes_since_yield = 0;
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    Ok(hex::encode(hasher.finalize()))
}
