let searchDebounce = null;

document.addEventListener("DOMContentLoaded", () => {
  const searchInput = document.getElementById("search-input");
  const btnClear = document.getElementById("btn-clear");
  const resultsContainer = document.getElementById("results-container");
  const searchMetrics = document.getElementById("search-metrics");
  const metricHits = document.getElementById("metric-hits");
  const metricTime = document.getElementById("metric-time");
  const metricTotalDocs = document.getElementById("metric-total-docs");
  const btnToggleFolders = document.getElementById("btn-toggle-folders");
  const btnCheckUpdate = document.getElementById("btn-check-update");
  const btnDuplicates = document.getElementById("btn-duplicates");
  const foldersDrawer = document.getElementById("folders-drawer");
  const duplicatesDrawer = document.getElementById("duplicates-drawer");
  const duplicatesContainer = document.getElementById("duplicates-container");
  const duplicatesSummary = document.getElementById("duplicates-summary");
  const btnRefreshDupes = document.getElementById("btn-refresh-dupes");
  const folderList = document.getElementById("folder-list");
  const folderCount = document.getElementById("folder-count");
  const newFolderInput = document.getElementById("new-folder-input");
  const btnAddFolder = document.getElementById("btn-add-folder");
  const btnReindex = document.getElementById("btn-reindex");
  const maxSizeInput = document.getElementById("max-size-input");
  const btnSaveMaxSize = document.getElementById("btn-save-max-size");
  const statusPill = document.getElementById("status-pill");
  const statusText = document.getElementById("status-text");
  const ocrStatusTag = document.getElementById("ocr-status-tag");

  // Initial load
  loadFolders();
  checkStatus();
  setInterval(checkStatus, 3000);

  // Search input typing
  searchInput.addEventListener("input", () => {
    const query = searchInput.value.trim();
    btnClear.classList.toggle("hidden", query.length === 0);

    clearTimeout(searchDebounce);
    if (!query) {
      resetResults();
      return;
    }

    searchDebounce = setTimeout(() => {
      performSearch(query);
    }, 180);
  });

  // Clear button
  btnClear.addEventListener("click", () => {
    searchInput.value = "";
    btnClear.classList.add("hidden");
    resetResults();
    searchInput.focus();
  });

  // Toggle Folders Drawer
  btnToggleFolders.addEventListener("click", () => {
    duplicatesDrawer.classList.add("hidden");
    foldersDrawer.classList.toggle("hidden");
  });

  // Toggle Duplicates Drawer
  btnDuplicates.addEventListener("click", () => {
    foldersDrawer.classList.add("hidden");
    duplicatesDrawer.classList.toggle("hidden");
    if (!duplicatesDrawer.classList.contains("hidden")) {
      loadDuplicates();
    }
  });

  btnRefreshDupes.addEventListener("click", loadDuplicates);

  if (window.location.hash === "#duplicates") {
    duplicatesDrawer.classList.remove("hidden");
    loadDuplicates();
  }

  // Check Updates
  btnCheckUpdate.addEventListener("click", async () => {
    btnCheckUpdate.disabled = true;
    btnCheckUpdate.textContent = "Checking...";
    try {
      const res = await fetch("/api/update/check");
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || "Update check failed");

      if (data.has_update && data.release) {
        const confirmUpdate = confirm(`New version ${data.release.tag_name} is available!\n\nRelease notes:\n${data.release.body || 'Bug fixes and performance improvements.'}\n\nWould you like to download and update now?`);
        if (confirmUpdate) {
          btnCheckUpdate.textContent = "Updating...";
          const applyRes = await fetch("/api/update/apply", { method: "POST" });
          const applyData = await applyRes.json();
          if (applyRes.ok) {
            alert(applyData.message || "Update applied successfully! Please restart DocSearch.");
          } else {
            alert(applyData.error || "Failed to apply update.");
          }
        }
      } else {
        alert(`DocSearch is up to date (v${data.current_version}).`);
      }
    } catch (e) {
      alert("Update check error: " + e.message);
    } finally {
      btnCheckUpdate.disabled = false;
      btnCheckUpdate.textContent = "⚡ Update";
    }
  });

  // Add Folder
  btnAddFolder.addEventListener("click", async () => {
    const path = newFolderInput.value.trim();
    if (!path) return;

    try {
      const res = await fetch("/api/folders", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path }),
      });
      const data = await res.json();
      if (res.ok) {
        newFolderInput.value = "";
        loadFolders();
      } else {
        alert(data.error || "Failed to add folder");
      }
    } catch (e) {
      alert("Network error: " + e.message);
    }
  });

  // Trigger Re-scan
  btnReindex.addEventListener("click", async () => {
    try {
      await fetch("/api/reindex", { method: "POST" });
      checkStatus();
    } catch (e) {
      console.error(e);
    }
  });

  // Save Max File Size
  btnSaveMaxSize.addEventListener("click", async () => {
    const mb = parseInt(maxSizeInput.value);
    if (isNaN(mb) || mb < 0) {
      alert("Please enter a valid file size limit in MB (or 0 for unlimited).");
      return;
    }

    try {
      const res = await fetch("/api/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ max_file_size_mb: mb }),
      });
      const data = await res.json();
      if (res.ok) {
        alert(`Updated max file size limit to ${data.max_file_size_mb} MB. Click "Trigger Re-scan" to apply.`);
      } else {
        alert(data.error || "Failed to update max file size");
      }
    } catch (e) {
      alert("Error saving settings: " + e.message);
    }
  });

  async function performSearch(query) {
    try {
      const res = await fetch(`/api/search?q=${encodeURIComponent(query)}&limit=30`);
      if (!res.ok) throw new Error("Search failed");
      const data = await res.json();

      metricHits.textContent = data.total_hits;
      metricTime.textContent = data.duration_ms;
      searchMetrics.classList.remove("hidden");

      renderResults(data.items);
    } catch (e) {
      console.error(e);
      resultsContainer.innerHTML = `<div class="empty-state"><p>Search error: ${e.message}</p></div>`;
    }
  }

  function renderResults(items) {
    if (!items || items.length === 0) {
      resultsContainer.innerHTML = `
        <div class="empty-state">
          <div class="empty-icon">🔎</div>
          <p>No matching documents found.</p>
          <p class="empty-sub">Try refining your search terms or verify your indexed folders.</p>
        </div>
      `;
      return;
    }

    resultsContainer.innerHTML = items.map((item) => {
      const icon = getFileIcon(item.extension);
      const sizeStr = formatBytes(item.size);
      const modStr = item.mod_time ? new Date(item.mod_time).toLocaleDateString() : "";
      const ocrBadge = item.is_ocr ? `<span class="tag-badge tag-ocr">OCR</span>` : "";
      
      const snippets = (item.highlights || [])
        .map(h => `<div class="result-snippet">${h}</div>`)
        .join("");

      return `
        <div class="result-card" data-path="${escapeHtml(item.path)}" title="Click to open file">
          <div class="result-header">
            <div class="result-title-group">
              <span class="file-icon">${icon}</span>
              <span class="result-filename">${escapeHtml(item.filename)}</span>
              <span class="tag-badge">${escapeHtml(item.extension || "FILE")}</span>
              ${ocrBadge}
            </div>
            <button class="btn btn-secondary" style="padding: 4px 8px; font-size: 11px;" onclick="event.stopPropagation(); openFolder('${escapeHtml(item.path)}')">Open in Folder</button>
          </div>
          <div class="result-path">${escapeHtml(item.path)}</div>
          ${snippets}
          <div class="result-meta">
            <span>Size: ${sizeStr}</span>
            <span>Modified: ${modStr}</span>
            <span>Score: ${item.score.toFixed(2)}</span>
          </div>
        </div>
      `;
    }).join("");

    // Attach click listeners to cards
    document.querySelectorAll(".result-card").forEach(card => {
      card.addEventListener("click", () => {
        openFile(card.getAttribute("data-path"));
      });
    });
  }

  function resetResults() {
    searchMetrics.classList.add("hidden");
    resultsContainer.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">📂</div>
        <p>Type a keyword above to instantly search through your selected folders.</p>
        <p class="empty-sub">Supports full-text queries, phrase search, and OCR extracted text.</p>
      </div>
    `;
  }

  async function loadFolders() {
    try {
      const res = await fetch("/api/folders");
      const folders = await res.json();
      folderCount.textContent = folders.length;

      if (folders.length === 0) {
        folderList.innerHTML = `<li class="folder-item"><em>No folders configured yet. Add one above!</em></li>`;
        return;
      }

      folderList.innerHTML = folders.map(f => `
        <li class="folder-item">
          <span class="folder-path">${escapeHtml(f)}</span>
          <button class="btn btn-danger" style="padding: 4px 8px; font-size: 11px;" onclick="removeFolder('${escapeHtml(f)}')">Remove</button>
        </li>
      `).join("");
    } catch (e) {
      console.error(e);
    }
  }

  window.removeFolder = async function(folderPath) {
    if (!confirm(`Stop indexing "${folderPath}"?`)) return;
    try {
      await fetch("/api/folders", {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: folderPath }),
      });
      loadFolders();
    } catch (e) {
      alert("Error removing folder: " + e.message);
    }
  };

  async function loadDuplicates() {
    duplicatesSummary.textContent = "Scanning...";
    duplicatesContainer.innerHTML = `<p class="empty-sub">Analyzing index for duplicate files...</p>`;

    try {
      const res = await fetch("/api/duplicates");
      if (!res.ok) throw new Error("Failed to scan duplicates");
      const data = await res.json();

      if (!data.groups || data.groups.length === 0) {
        duplicatesSummary.textContent = "0 Duplicates";
        duplicatesSummary.className = "badge";
        duplicatesContainer.innerHTML = `
          <div class="empty-state" style="padding: 24px 0;">
            <p>✅ No duplicate documents found across your indexed folders.</p>
          </div>
        `;
        return;
      }

      const wastedStr = formatBytes(data.total_wasted_bytes);
      duplicatesSummary.textContent = `${data.total_duplicates} duplicates (${wastedStr} wasted)`;
      duplicatesSummary.className = "badge badge-wasted";

      duplicatesContainer.innerHTML = data.groups.map((group, idx) => {
        const groupSize = formatBytes(group.size);
        const groupWasted = formatBytes(group.wasted_bytes);
        const hashShort = group.hash ? group.hash.substring(0, 10) + "..." : "";

        const filesHtml = group.files.map((file) => `
          <li class="dupe-file-item">
            <div class="dupe-file-info">
              <span class="dupe-file-name">${escapeHtml(file.filename)}</span>
              <span class="dupe-file-path">${escapeHtml(file.path)}</span>
            </div>
            <div class="dupe-actions">
              <button class="btn btn-secondary" style="padding: 3px 6px; font-size: 11px;" onclick="openFile('${escapeHtml(file.path)}')">Open</button>
              <button class="btn btn-secondary" style="padding: 3px 6px; font-size: 11px;" onclick="openFolder('${escapeHtml(file.path)}')">Folder</button>
              <button class="btn btn-danger" style="padding: 3px 6px; font-size: 11px;" onclick="deleteFile('${escapeHtml(file.path)}')">Delete</button>
            </div>
          </li>
        `).join("");

        return `
          <div class="duplicate-group-card">
            <div class="dupe-group-header">
              <span><strong>Group #${idx + 1}</strong> (${group.count} copies) • Hash: ${hashShort}</span>
              <div class="dupe-tags">
                <span class="badge">Size: ${groupSize}</span>
                <span class="badge badge-wasted">Wasted: ${groupWasted}</span>
              </div>
            </div>
            <ul class="dupe-file-list">
              ${filesHtml}
            </ul>
          </div>
        `;
      }).join("");
    } catch (e) {
      duplicatesContainer.innerHTML = `<p class="empty-sub" style="color: #ef4444;">Error loading duplicates: ${e.message}</p>`;
    }
  }

  window.deleteFile = async function(filePath) {
    if (!confirm(`Are you sure you want to permanently delete:\n"${filePath}"?`)) {
      return;
    }

    try {
      const res = await fetch("/api/file", {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: filePath }),
      });
      const data = await res.json();
      if (res.ok) {
        loadDuplicates();
        checkStatus();
      } else {
        alert(data.error || "Failed to delete file");
      }
    } catch (e) {
      alert("Error deleting file: " + e.message);
    }
  };

  async function checkStatus() {
    try {
      const res = await fetch("/api/status");
      const st = await res.json();

      if (st.is_indexing) {
        statusPill.classList.add("indexing");
        statusText.textContent = `Indexing (${st.indexed_count}/${st.total_files})`;
      } else {
        statusPill.classList.remove("indexing");
        statusText.textContent = `Indexed (${st.doc_count} docs)`;
      }

      metricTotalDocs.textContent = `Total Documents: ${st.doc_count}`;
      if (document.activeElement !== maxSizeInput && st.max_file_size_mb !== undefined) {
        maxSizeInput.value = st.max_file_size_mb;
      }
      ocrStatusTag.textContent = st.ocr_available ? "OCR Engine: Available" : "OCR Engine: Not Installed (Tesseract recommended)";
      if (st.ocr_available) {
        ocrStatusTag.style.color = "#10b981";
      } else {
        ocrStatusTag.style.color = "#94a3b8";
      }
    } catch (e) {
      console.error(e);
    }
  }

  window.openFile = function(path) {
    fetch("/api/open", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path, reveal: false }),
    });
  };

  window.openFolder = function(path) {
    fetch("/api/open", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path, reveal: true }),
    });
  };

  function getFileIcon(ext) {
    const e = (ext || "").toLowerCase();
    if (e === ".pdf") return "📄";
    if (e === ".docx" || e === ".doc") return "📝";
    if (e === ".txt" || e === ".md" || e === ".markdown") return "📋";
    if (e === ".png" || e === ".jpg" || e === ".jpeg") return "🖼️";
    if (e === ".csv" || e === ".xlsx") return "📊";
    if ([".go", ".py", ".js", ".ts", ".html", ".css", ".sh"].includes(e)) return "💻";
    return "📁";
  }

  function formatBytes(bytes) {
    if (!bytes || bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  }

  function escapeHtml(str) {
    if (!str) return "";
    return str
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }
});
