package desktop

import (
	"io/fs"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
)

// Run starts the Wails native desktop application window.
func Run(app *App, assets fs.FS) error {
	return wails.Run(&options.App{
		Title:             "DocSearch",
		Width:             980,
		Height:            700,
		MinWidth:          640,
		MinHeight:         480,
		HideWindowOnClose: true,
		AssetServer: &assetserver.Options{
			Assets:  assets,
			Handler: app.HTTPHandler(),
		},
		BackgroundColour: &options.RGBA{R: 18, G: 20, B: 26, A: 255},
		OnStartup:        app.Startup,
		Bind: []interface{}{
			app,
		},
	})
}
