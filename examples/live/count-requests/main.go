// Command count-requests runs a command with PALETTE_API set to a local address. It sends each
// request of the command on to Palette, and it writes one line for each request to a file: the
// method and the path. The line has no query and no header, so the key is not in it. The live
// example uses it to count the requests that a cycle of the observer makes.
//
//	count-requests -log FILE [-target URL] -- COMMAND [ARG...]
//
// The program exits with the code of the command, and with 2 when it cannot start.
package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"os"
	"os/exec"
	"sync"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdin, os.Stdout, os.Stderr))
}

func run(args []string, stdin io.Reader, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("count-requests", flag.ContinueOnError)
	fs.SetOutput(stderr)
	target := fs.String("target", "https://api.spectrocloud.com", "the address of Palette")
	logPath := fs.String("log", "", "the file for the lines of the requests (required)")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	command := fs.Args()
	if *logPath == "" || len(command) == 0 {
		fmt.Fprintln(stderr, "usage: count-requests -log FILE [-target URL] -- COMMAND [ARG...]")
		return 2
	}
	upstream, err := url.Parse(*target)
	if err != nil || (upstream.Scheme != "http" && upstream.Scheme != "https") || upstream.Host == "" {
		fmt.Fprintln(stderr, "count-requests: -target needs an http or https address")
		return 2
	}
	log, err := os.Create(*logPath)
	if err != nil {
		fmt.Fprintln(stderr, "count-requests: the log cannot be written")
		return 2
	}
	defer log.Close()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		fmt.Fprintln(stderr, "count-requests: no local port")
		return 2
	}
	server := &http.Server{Handler: counted(forward(upstream), log)}
	go func() { _ = server.Serve(ln) }()
	defer server.Close()

	cmd := exec.Command(command[0], command[1:]...)
	cmd.Stdin, cmd.Stdout, cmd.Stderr = stdin, stdout, stderr
	cmd.Env = append(os.Environ(), "PALETTE_API=http://"+ln.Addr().String())
	err = cmd.Run()
	var exit *exec.ExitError
	switch {
	case errors.As(err, &exit):
		return exit.ExitCode()
	case err != nil:
		fmt.Fprintln(stderr, "count-requests: the command cannot start")
		return 2
	}
	return 0
}

// forward sends each request to upstream with the Host of upstream.
func forward(upstream *url.URL) http.Handler {
	return &httputil.ReverseProxy{Rewrite: func(r *httputil.ProxyRequest) { r.SetURL(upstream) }}
}

// counted writes one line for each request before it hands the request on.
func counted(next http.Handler, log io.Writer) http.Handler {
	var mu sync.Mutex
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		fmt.Fprintf(log, "%s %s\n", r.Method, r.URL.Path)
		mu.Unlock()
		next.ServeHTTP(w, r)
	})
}
