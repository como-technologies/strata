package api

import (
	"net/http"
	"testing"
	"testing/fstest"
)

func TestPageServesEachFileWithTheTypeOfItsKind(t *testing.T) {
	h := (&Server{Source: stub{}, Page: Page(fstest.MapFS{
		"index.html":       {Data: []byte("<title>page</title>")},
		"app.js":           {Data: []byte("1")},
		"app.css":          {Data: []byte("a{}")},
		"fonts/a.woff2":    {Data: []byte("font")},
		"LICENSES.md":      {Data: []byte("# licenses")},
		"secret.json":      {Data: []byte("{}")},
		"fonts/readme.txt": {Data: []byte("text")},
	})}).Handler()
	for _, c := range []struct {
		path, typ string
		status    int
	}{
		{"/", "text/html; charset=utf-8", 200},
		{"/index.html", "text/html; charset=utf-8", 200},
		{"/app.js", "text/javascript; charset=utf-8", 200},
		{"/app.css", "text/css; charset=utf-8", 200},
		{"/fonts/a.woff2", "font/woff2", 200},
		{"/LICENSES.md", "text/markdown; charset=utf-8", 200},
		// A kind that has no type is not served, and nor is a file that does not exist.
		{"/secret.json", "application/json", 404},
		{"/fonts/readme.txt", "application/json", 404},
		{"/missing.js", "application/json", 404},
		{"/fonts", "application/json", 404},
		{"/fonts/", "application/json", 404},
		{"/index.html/", "application/json", 404},
	} {
		rec := get(h, c.path)
		if rec.Code != c.status || rec.Header().Get("Content-Type") != c.typ {
			t.Errorf("%s: status %d, type %q, want %d and %q", c.path, rec.Code, rec.Header().Get("Content-Type"), c.status, c.typ)
		}
		if rec.Header().Get("Content-Security-Policy") == "" || rec.Header().Get("Cache-Control") != "no-store" {
			t.Errorf("%s: the headers of the server are missing", c.path)
		}
	}
}

// A browser asks for /favicon.ico with no link in the page. A 404 is an error in its console, so the
// page answers 204.
func TestPageAnswersTheIconPathWithNoContent(t *testing.T) {
	h := (&Server{Source: stub{}, Page: Page(fstest.MapFS{})}).Handler()
	if rec := get(h, "/favicon.ico"); rec.Code != http.StatusNoContent || rec.Body.Len() != 0 {
		t.Errorf("status %d, %d bytes", rec.Code, rec.Body.Len())
	}
}

// The mux cleans a path with . or .. in it and redirects, and the page serves only a name in its files.
func TestPageNeverLeavesItsFiles(t *testing.T) {
	h := (&Server{Source: stub{}, Page: Page(fstest.MapFS{"app.js": {Data: []byte("1")}})}).Handler()
	for _, path := range []string{"/../app.js", "/./app.js", "/%2e%2e/app.js", "/fonts/../app.js", "//app.js", "/app.js%00.css"} {
		if rec := get(h, path); rec.Code == http.StatusOK {
			t.Errorf("%s gave 200", path)
		}
	}
}
