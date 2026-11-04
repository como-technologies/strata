// Command devserver serves the dashboard of the observer from test data, so that a person sees each
// state of the page with no strata binary, no records, and no Palette. It serves the files of
// internal/web and the answers of the sets in internal/web/testdata/api, through the real server of
// internal/api. So the headers are those of the real server, and the console of the browser shows what
// the real page shows.
//
//	go -C observer run ./internal/web/devserver -state ok
package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"os"
	"os/signal"
	"slices"
	"strings"
	"syscall"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/web"
	"github.com/como-technologies/strata/observer/internal/web/devdata"
)

// defaultData is the directory of the sets, from the directory observer/.
const defaultData = "internal/web/testdata/api"

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	code := run(ctx, os.Args[1:], os.Stderr)
	stop()
	os.Exit(code)
}

// stateNames lists the sets for the help text.
func stateNames() string {
	names := make([]string, 0, len(devdata.States))
	for _, s := range devdata.States {
		names = append(names, string(s))
	}
	return strings.Join(names, ", ")
}

// run serves a set until ctx ends. It gives 2 when a flag is wrong, when the set cannot be read, or
// when the server fails.
func run(ctx context.Context, args []string, stderr io.Writer) int {
	fs := flag.NewFlagSet("devserver", flag.ContinueOnError)
	fs.SetOutput(stderr)
	state := fs.String("state", string(devdata.StateOK), "the set of test data: "+stateNames())
	listen := fs.String("listen", api.DefaultListen, "a loopback IP address and a port")
	data := fs.String("data", defaultData, "the directory of the sets")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if !slices.Contains(devdata.States, devdata.State(*state)) {
		fmt.Fprintf(stderr, "devserver: --state: %q is no set; the sets are %s\n", *state, stateNames())
		return 2
	}
	set, err := devdata.Load(os.DirFS(*data), devdata.State(*state), time.Now())
	if err != nil {
		fmt.Fprintln(stderr, "devserver:", err)
		return 2
	}
	files, err := web.Files()
	if err != nil {
		fmt.Fprintln(stderr, "devserver:", err)
		return 2
	}
	ln, err := api.Listen(*listen)
	if err != nil {
		fmt.Fprintln(stderr, "devserver: --listen:", err)
		return 2
	}
	defer ln.Close()
	log := slog.New(slog.NewTextHandler(stderr, nil))
	log.Info("serving test data", "state", *state, "page", set.Cluster.Page)
	if err := (&api.Server{Source: set, Page: api.Page(files)}).Serve(ctx, ln, log); err != nil {
		fmt.Fprintln(stderr, "devserver: the server stopped:", err)
		return 2
	}
	return 0
}
