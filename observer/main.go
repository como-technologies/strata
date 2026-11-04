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
	"syscall"
	"time"

	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// version is set by the build with -ldflags "-X main.version=...".
var version = "dev"

// processTimeout is the time limit of one strata process.
const processTimeout = 10 * time.Minute

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

const usage = "usage: strata-observer version | once --cluster-uid UID --records DIR --work DIR --state DIR"

// run gives the exit code. The codes have the meaning that they have in strata: 1 says that a
// finding exists, and 2 is an error.
func run(args []string, stdout, stderr io.Writer) int {
	switch {
	case len(args) == 1 && args[0] == "version":
		fmt.Fprintln(stdout, "strata-observer", version)
		return 0
	case len(args) >= 1 && args[0] == "once":
		return once(args[1:], stdout, stderr)
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

// parse reads the flags and refuses a missing flag or a period that is not positive.
func (o *options) parse(name string, args []string, stderr io.Writer) error {
	fs := flag.NewFlagSet(name, flag.ContinueOnError)
	fs.SetOutput(stderr)
	o.flags(fs)
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
	return nil
}

// once runs one content cycle and one versions cycle, and prints the summary. It exits 1 when a
// finding exists, as `strata verify` does. It exits 2 when no finding exists and a cycle failed, or
// when the flags are wrong.
func once(args []string, stdout, stderr io.Writer) int {
	var o options
	if err := o.parse("once", args, stderr); err != nil {
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 2
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	log := slog.New(slog.NewTextHandler(stderr, nil))
	strata := check.InDir{Runner: runner.Runner{Binary: o.strata, Timeout: processTimeout}}
	summary, err := check.New(o.Config, strata, log).Once(ctx)
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
