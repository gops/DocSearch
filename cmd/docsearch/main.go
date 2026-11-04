package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"docsearch/pkg/config"
	"docsearch/pkg/crawler"
	"docsearch/pkg/extractor"
	"docsearch/pkg/indexer"
	"docsearch/pkg/server"
	"docsearch/pkg/tray"
	"docsearch/pkg/updater"
)

var (
	Version = "0.1.5"
	Banner  = `
  ___             ____                      _     
 |   \  ___  __  / ___|  ___  __ _ _ _ __ _| |_   
 | |) |/ _ \/ _| \___ \ / _ \/ _` + "`" + ` | '_/ _| ' \  
 |___/ \___/\__| |____/ \___/\__,_|_| \__|_||_| 
  Local Cross-Platform Document Search Engine (Go + Bluge)
`
)

func main() {
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "search":
			handleCLISearch(os.Args[2:])
			return
		case "add":
			handleCLIAddFolder(os.Args[2:])
			return
		case "list":
			handleCLIList()
			return
		case "reindex":
			handleCLIReindex()
			return
		case "duplicates", "dupes":
			handleCLIDuplicates()
			return
		case "check-update":
			handleCLICheckUpdate()
			return
		case "update":
			handleCLIApplyUpdate()
			return
		case "-version", "--version", "version":
			fmt.Printf("DocSearch v%s (MIT License)\n", Version)
			return
		}
	}

	// Daemon / System Tray mode
	configPath := flag.String("config", "", "Path to custom config JSON file")
	portFlag := flag.Int("port", 0, "Override HTTP server port")
	headlessFlag := flag.Bool("headless", false, "Run in background without system tray icon")
	maxSizeFlag := flag.Int("max-size-mb", 0, "Skip files larger than this size in megabytes (default 25)")
	openFlag := flag.Bool("open", false, "Open search interface in default browser on startup")
	flag.Parse()

	fmt.Print(Banner)
	log.Printf("[Init] Starting DocSearch v%s...\n", Version)

	cfg, err := config.Load(*configPath)
	if err != nil {
		log.Fatalf("[Init] Failed to load configuration: %v\n", err)
	}

	if *portFlag > 0 {
		cfg.ServerPort = *portFlag
	}
	if *maxSizeFlag > 0 {
		cfg.SetMaxFileSizeMB(*maxSizeFlag)
	}

	log.Printf("[Init] Using index path: %s\n", cfg.IndexPath)
	log.Printf("[Init] Max file size limit: %d MB\n", cfg.MaxFileSizeMB)
	log.Printf("[Init] Configured folders to index: %v\n", cfg.GetFolders())

	// Initialize Bluge Indexer
	idx, err := indexer.Open(cfg.IndexPath)
	if err != nil {
		log.Fatalf("[Init] Failed to open indexer: %v\n", err)
	}
	defer idx.Close()

	// Initialize Document Extractor
	ext := extractor.NewManager(cfg.OCREnabled, cfg.OCRCommand)
	if extractor.IsOCRAvailable(cfg.OCRCommand) {
		log.Printf("[Init] OCR engine %q detected and active.\n", cfg.OCRCommand)
	} else {
		log.Printf("[Init] OCR engine %q not detected in PATH. Image/Scanned PDF OCR will be skipped.\n", cfg.OCRCommand)
	}

	// Initialize Crawler & Watcher
	c, err := crawler.New(cfg, idx, ext)
	if err != nil {
		log.Fatalf("[Init] Failed to initialize crawler: %v\n", err)
	}
	defer c.Stop()

	// Start crawler
	c.Start()

	// Start Web UI Server
	srv := server.NewServer(cfg, idx, c, Version)
	go func() {
		if err := srv.Start(); err != nil {
			log.Fatalf("[Server] HTTP server error: %v\n", err)
		}
	}()

	if *openFlag {
		tray.OpenBrowser(fmt.Sprintf("http://127.0.0.1:%d", cfg.ServerPort))
	}

	// Cleanup callback
	cleanup := func() {
		log.Println("\n[Shutdown] Shutting down DocSearch...")
		c.Stop()
		_ = idx.Close()
		os.Exit(0)
	}

	// Determine if system tray should be enabled
	useTray := !*headlessFlag && tray.HasDisplayEnvironment()

	if useTray {
		log.Println("[Init] System tray active. DocSearch is accessible from your system tray.")
		t := tray.New(cfg, idx, c, Version, cleanup)
		t.Run() // Blocks main thread
	} else {
		log.Println("[Init] Running in headless mode. Press Ctrl+C to stop.")
		sigChan := make(chan os.Signal, 1)
		signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)
		<-sigChan
		cleanup()
	}
}

func handleCLISearch(args []string) {
	if len(args) == 0 {
		fmt.Println("Usage: docsearch search <query>")
		os.Exit(1)
	}

	query := args[0]
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	idx, err := indexer.Open(cfg.IndexPath)
	if err != nil {
		log.Fatalf("Error opening index: %v", err)
	}
	defer idx.Close()

	res, err := idx.Search(query, 10)
	if err != nil {
		log.Fatalf("Search failed: %v", err)
	}

	fmt.Printf("\nFound %d matches for %q in %d ms:\n\n", res.TotalHits, query, res.DurationMs)
	for i, hit := range res.Items {
		fmt.Printf("[%d] %s (%s)\n", i+1, hit.Filename, hit.Path)
		for _, h := range hit.Highlights {
			fmt.Printf("    -> %s\n", h)
		}
		fmt.Println()
	}
}

func handleCLIAddFolder(args []string) {
	if len(args) == 0 {
		fmt.Println("Usage: docsearch add <path-to-folder>")
		os.Exit(1)
	}

	folder := args[0]
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	added, err := cfg.AddFolder(folder)
	if err != nil {
		log.Fatalf("Error: %v", err)
	}

	if added {
		if err := cfg.Save(); err != nil {
			log.Fatalf("Failed to save config: %v", err)
		}
		fmt.Printf("Successfully added folder: %s\n", folder)
	} else {
		fmt.Printf("Folder already present in index list: %s\n", folder)
	}
}

func handleCLIList() {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	folders := cfg.GetFolders()
	fmt.Printf("\nConfigured Indexed Folders (%d):\n", len(folders))
	for i, f := range folders {
		fmt.Printf("  [%d] %s\n", i+1, f)
	}

	maxSizeStr := fmt.Sprintf("%d MB", cfg.MaxFileSizeMB)
	if cfg.MaxFileSizeMB <= 0 {
		maxSizeStr = "Unlimited (0 MB)"
	}
	fmt.Printf("\nMax File Size Limit: %s (larger files are skipped)\n", maxSizeStr)

	idx, err := indexer.Open(cfg.IndexPath)
	if err == nil {
		defer idx.Close()
		count, _ := idx.DocCount()
		fmt.Printf("Total Indexed Documents: %d\n\n", count)
	}
}

func handleCLIReindex() {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	idx, err := indexer.Open(cfg.IndexPath)
	if err != nil {
		log.Fatalf("Error opening index: %v", err)
	}
	defer idx.Close()

	ext := extractor.NewManager(cfg.OCREnabled, cfg.OCRCommand)
	c, err := crawler.New(cfg, idx, ext)
	if err != nil {
		log.Fatalf("Error initializing crawler: %v", err)
	}
	defer c.Stop()

	fmt.Println("Starting re-indexing...")
	c.ScanAll()
	count, _ := idx.DocCount()
	fmt.Printf("Re-indexing complete. Current total documents: %d\n", count)
}

func handleCLIDuplicates() {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	idx, err := indexer.Open(cfg.IndexPath)
	if err != nil {
		log.Fatalf("Error opening index: %v", err)
	}
	defer idx.Close()

	report, err := idx.FindDuplicates()
	if err != nil {
		log.Fatalf("Failed to scan for duplicates: %v", err)
	}

	if report.TotalGroups == 0 {
		fmt.Println("\n✅ No duplicate files found across your indexed folders.")
		return
	}

	wastedStr := formatByteSize(report.TotalWastedBytes)
	fmt.Printf("\n📑 Found %d duplicate groups (%d redundant copies, %s wasted disk space):\n\n",
		report.TotalGroups, report.TotalDuplicates, wastedStr)

	for i, group := range report.Groups {
		groupWasted := formatByteSize(group.WastedBytes)
		fileSize := formatByteSize(group.Size)
		hashSnippet := group.Hash
		if len(hashSnippet) > 12 {
			hashSnippet = hashSnippet[:12] + "..."
		}

		fmt.Printf("[Group %d] SHA-256: %s | Size: %s | Wasted: %s (%d copies)\n",
			i+1, hashSnippet, fileSize, groupWasted, group.Count)

		for j, f := range group.Files {
			modStr := ""
			if !f.ModTime.IsZero() {
				modStr = fmt.Sprintf(" (Modified: %s)", f.ModTime.Format("2006-01-02 15:04"))
			}
			fmt.Printf("   [%d] %s%s\n", j+1, f.Path, modStr)
		}
		fmt.Println()
	}
}

func formatByteSize(bytes int64) string {
	if bytes <= 0 {
		return "0 B"
	}
	const k = 1024
	sizes := []string{"B", "KB", "MB", "GB", "TB"}
	i := 0
	val := float64(bytes)
	for val >= 1024 && i < len(sizes)-1 {
		val /= 1024
		i++
	}
	return fmt.Sprintf("%.1f %s", val, sizes[i])
}

func handleCLICheckUpdate() {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	repo := cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	fmt.Printf("Checking for updates from https://github.com/%s (current: v%s)...\n", repo, Version)
	rel, hasUpdate, err := updater.CheckLatestRelease(repo, Version)
	if err != nil {
		fmt.Printf("Update check failed: %v\n", err)
		return
	}

	if hasUpdate {
		fmt.Printf("\n✨ New version available: %s\n", rel.TagName)
		fmt.Printf("Release Notes:\n%s\n", rel.Body)
		fmt.Printf("Download URL: %s\n", rel.HTMLURL)
		fmt.Println("To apply this update automatically, run: docsearch update")
	} else {
		fmt.Printf("You are running the latest version (v%s).\n", Version)
	}
}

func handleCLIApplyUpdate() {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("Error loading config: %v", err)
	}

	repo := cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	fmt.Printf("Checking for updates from https://github.com/%s...\n", repo)
	rel, hasUpdate, err := updater.CheckLatestRelease(repo, Version)
	if err != nil {
		log.Fatalf("Update check failed: %v", err)
	}

	if !hasUpdate || rel == nil {
		fmt.Printf("Already up to date (v%s).\n", Version)
		return
	}

	fmt.Printf("Downloading and applying %s (%s)...\n", rel.TagName, rel.AssetName)
	if err := updater.DownloadAndApply(rel); err != nil {
		log.Fatalf("Failed to apply update: %v", err)
	}

	fmt.Printf("✅ Successfully updated to %s! Please restart DocSearch.\n", rel.TagName)
}
