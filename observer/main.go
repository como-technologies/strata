// Command strata-observer runs the strata binary on two timers and serves the answers on a page.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/web"
)

// version is set by the build with -ldflags "-X main.version=...".
var version = "dev"

// processTimeout is the time limit of one strata process.
const processTimeout = 10 * time.Minute

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	code := runContext(ctx, os.Args[1:], os.Stdout, os.Stderr)
	stop()
	os.Exit(code)
}

const usage = "usage: strata-observer version | once FLAGS | serve FLAGS [--listen ADDR] | health [--listen ADDR]; FLAGS are --cluster-uid UID --records DIR --work DIR --state DIR"

// runContext gives the exit code. The codes have the meaning that they have in strata: 1 says that a
// finding exists, and 2 is an error. For `health`, 1 says that the server is not healthy. ctx stops
// a run of `serve`, and ends a cycle.
func runContext(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	if len(args) == 1 && args[0] == "version" {
		fmt.Fprintln(stdout, "strata-observer", version)
		return 0
	}
	if len(args) >= 1 {
		switch args[0] {
		case "once":
			return once(ctx, args[1:], stdout, stderr)
		case "serve":
			return serve(ctx, args[1:], stderr)
		case "health":
			return health(ctx, args[1:], stderr)
		}
	}
	fmt.Fprintln(stderr, usage)
	return 2
}

// options holds the flags that `serve` and `once` share.
type options struct {
	strata string
	check.Config
}

func (o *options) flags(fs *flag.FlagSet) {
	fs.StringVar(&o.strata, "strata", "strata", "the strata binary; the default is the one on PATH")
	fs.StringVar(&o.ClusterUID, "cluster-uid", "", "the cluster to watch (required)")
	fs.StringVar(&o.Records, "records", "", "the directory of *.json.gz and meta.json (required)")
	fs.StringVar(&o.Work, "work", "", "a volume in memory for the files of a cycle (required)")
	fs.StringVar(&o.State, "state", "", "a directory for state.json (required)")
	fs.DurationVar(&o.ContentEvery, "content-every", 15*time.Minute, "the period of a content cycle")
	fs.DurationVar(&o.VersionsEvery, "versions-every", 6*time.Hour, "the period of a versions cycle")
}

// parse reads the flags and refuses a missing flag or a period that is not positive. A command with
// more flags adds them in extra.
func (o *options) parse(name string, args []string, stderr io.Writer, extra ...func(*flag.FlagSet)) error {
	fs := flag.NewFlagSet(name, flag.ContinueOnError)
	fs.SetOutput(stderr)
	o.flags(fs)
	for _, add := range extra {
		add(fs)
	}
	if err := fs.Parse(args); err != nil {
		return err
	}
	for _, f := range []struct{ name, value string }{{"cluster-uid", o.ClusterUID}, {"records", o.Records}, {"work", o.Work}, {"state", o.State}} {
		if f.value == "" {
			return fmt.Errorf("the flag --%s is required", f.name)
		}
	}
	if o.ContentEvery <= 0 || o.VersionsEvery <= 0 {
		return errors.New("a period must be more than zero")
	}
	return o.absoluteStrata()
}

// absoluteStrata makes a path of the strata binary absolute, from the directory of the observer. A
// process runs in the directory of its cycle, and Go reads a relative path from there, so a relative
// path would fail in each cycle. A name with no separator stays: the lookup on PATH makes it absolute.
func (o *options) absoluteStrata() error {
	if !strings.ContainsRune(o.strata, filepath.Separator) {
		return nil
	}
	abs, err := filepath.Abs(o.strata)
	if err != nil {
		return fmt.Errorf("the flag --strata: %w", err)
	}
	o.strata = abs
	return nil
}

// observer makes the observer of the flags. It runs the strata binary of the flags.
func (o *options) observer(log *slog.Logger) *check.Observer {
	strata := check.InDir{Runner: runner.Runner{Binary: o.strata, Timeout: processTimeout}}
	return check.New(o.Config, strata, log)
}

// once runs one content cycle and one versions cycle, and prints the summary. It exits 1 when a
// finding exists, as `strata verify` does. It exits 2 when no finding exists and a cycle failed, or
// when the flags are wrong.
func once(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	var o options
	if err := o.parse("once", args, stderr); err != nil {
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 2
	}
	log := slog.New(slog.NewTextHandler(stderr, nil))
	summary, err := o.observer(log).Once(ctx)
	enc := json.NewEncoder(stdout)
	enc.SetIndent("", "  ")
	enc.SetEscapeHTML(false)
	if encErr := enc.Encode(summary); encErr != nil {
		fmt.Fprintln(stderr, "strata-observer: the summary cannot be written")
		return 2
	}
	// A finding is a change, so it wins over a failed cycle: the exit 2 never says "no change".
	switch {
	case summary.Counts.Findings > 0:
		return 1
	case err != nil:
		return 2
	}
	return 0
}

// serve runs the two timers and serves the answers on a loopback address, until ctx ends. It exits 2
// when the flags are wrong, when the address is not a loopback address, or when the server fails.
func serve(ctx context.Context, args []string, stderr io.Writer) int {
	var o options
	listen := api.DefaultListen
	err := o.parse("serve", args, stderr, func(fs *flag.FlagSet) {
		fs.StringVar(&listen, "listen", api.DefaultListen, "a loopback IP address and a port; no other address is allowed")
	})
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 2
	}
	ln, err := api.Listen(listen)
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer: --listen:", err)
		return 2
	}
	defer ln.Close()
	files, err := web.Files()
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer: the files of the page cannot be read")
		return 2
	}
	log := slog.New(slog.NewTextHandler(stderr, nil))
	obs := o.observer(log)
	ctx, stop := context.WithCancel(ctx)
	defer stop()
	var worker sync.WaitGroup
	worker.Add(1)
	go func() {
		defer worker.Done()
		obs.Run(ctx)
	}()
	err = (&api.Server{Source: obs, Page: api.Page(files)}).Serve(ctx, ln, log)
	stop()
	worker.Wait()
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer: the server stopped:", err)
		return 2
	}
	return 0
}

// health asks the server for its health route, and exits 0 when it answers 200. It exits 1 when it does not,
// and 2 when the flags are wrong. The probes of the Deployment run it.
func health(ctx context.Context, args []string, stderr io.Writer) int {
	fs := flag.NewFlagSet("health", flag.ContinueOnError)
	fs.SetOutput(stderr)
	listen := fs.String("listen", api.DefaultListen, "the loopback address of the server")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	err := api.Health(ctx, *listen)
	switch {
	case errors.Is(err, api.ErrNotLoopback):
		fmt.Fprintln(stderr, "strata-observer: --listen:", err)
		return 2
	case err != nil:
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 1
	}
	return 0
}
