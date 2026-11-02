package frontend

import "embed"

// Assets embeds the desktop application frontend files.
//
//go:embed *
var Assets embed.FS
