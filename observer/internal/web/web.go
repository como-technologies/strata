// Package web holds the files of the dashboard: the page, its script, its style, and its fonts.
//
// The files are in assets/, and the program carries them. The page loads its script, its style, and
// its fonts from the server itself and from nowhere else, because the policy of the server is
// `script-src 'self'; style-src 'self'; font-src 'self'`, and a cluster can have no egress. So the
// page has no inline style, no inline script, and no address of another host. A test of this package
// fails when a file breaks one of these rules. internal/api serves the files: see api.Page.
package web

import (
	"embed"
	"io/fs"
)

//go:embed assets
var assets embed.FS

// Files gives the files of the page, with index.html at the root.
func Files() (fs.FS, error) {
	return fs.Sub(assets, "assets")
}
