package api

import (
	"io/fs"
	"net/http"
	"path"
	"strings"
)

// pageTypes gives the Content-Type of each kind of file that the page has. The server sends the type
// itself and does not guess it from the bytes. A file of another kind is not served.
var pageTypes = map[string]string{
	".html":  "text/html; charset=utf-8",
	".js":    "text/javascript; charset=utf-8",
	".css":   "text/css; charset=utf-8",
	".woff2": "font/woff2",
	".md":    "text/markdown; charset=utf-8",
}

// pageIndex is the file that the route `/` serves.
const pageIndex = "index.html"

// pageIcon is the path that a browser asks for with no link in the page.
const pageIcon = "favicon.ico"

// Page serves the files of the page from files. The route `/` gives index.html. A path gives the file
// with that name, if the file has a type in pageTypes. Any other path gets 404. The page has no icon,
// so the path of the icon gets 204, and the console of the browser shows no error.
func Page(files fs.FS) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		name := strings.TrimPrefix(r.URL.Path, "/")
		if name == "" {
			name = pageIndex
		}
		if name == pageIcon {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		typ, ok := pageTypes[path.Ext(name)]
		if !ok || !fs.ValidPath(name) {
			notFound(w, r)
			return
		}
		body, err := fs.ReadFile(files, name)
		if err != nil {
			notFound(w, r)
			return
		}
		w.Header().Set("Content-Type", typ)
		_, _ = w.Write(body)
	})
}
