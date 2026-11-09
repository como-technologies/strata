// Command strata-observer runs the strata binary on two timers and serves the answers on a page.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"github.com/como-technologies/strata/observer/internal/seed"
	"io"
	"log/slog"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/check"
	"github.com/como-technologies/strata/observer/internal/kube"
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

const usage = "usage: strata-observer version | once FLAGS | serve FLAGS [--listen ADDR] | health [--listen ADDR] | seed --cluster-uid UID --work DIR [--namespace NS] [--strata FILE] [--kube-dir DIR]; FLAGS are --cluster-uid UID --records DIR --work DIR --state DIR [--read-pods [--kube-dir DIR]] [--alerts FILE [--digest-every DURATION]]"

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
		case "seed":
			return seedRecords(ctx, args[1:], stdout, stderr)
		}
	}
	fmt.Fprintln(stderr, usage)
	return 2
}

// options holds the flags that `serve` and `once` share.
type options struct {
	strata      string
	readPods    bool
	kubeDir     string
	alerts      string
	allowHTTP   bool
	digestEvery time.Duration
	// rules is the file of the alert rules, after the checks. It is nil when the flag --alerts is not set.
	rules *alert.Rules
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
	fs.BoolVar(&o.readPods, "read-pods", false, "read the pods at each content cycle, for the digest of each image; needs the role of deploy/observer/pods")
	fs.StringVar(&o.kubeDir, "kube-dir", kube.ServiceDir, "the directory with the files token and ca.crt, for --read-pods")
	fs.StringVar(&o.alerts, "alerts", "", "the file of the alert rules, in JSON; serve sends them, once only checks the file; with no file, nothing sends")
	fs.BoolVar(&o.allowHTTP, "alerts-allow-http", false, "allow an http address for a webhook; for tests only")
	fs.DurationVar(&o.digestEvery, "digest-every", 7*24*time.Hour, "the period of a digest of the alerts")
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
	if o.ContentEvery <= 0 || o.VersionsEvery <= 0 || o.digestEvery <= 0 {
		return errors.New("a period must be more than zero")
	}
	if err := o.loadRules(); err != nil {
		return err
	}
	return o.absoluteStrata()
}

// loadRules reads the file of the alert rules, when the flag names one. A fault stops the start, with the line of the fault.
func (o *options) loadRules() error {
	if o.alerts == "" {
		return nil
	}
	rules, err := alert.Load(o.alerts)
	if err != nil {
		return fmt.Errorf("the flag --alerts: %w", err)
	}
	o.rules = &rules
	return nil
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

// observer makes the observer of the flags. It runs the strata binary of the flags. With --read-pods it
// reads the pods too, and it fails when the variables of the API server are not set. Without the flag, the
// observer makes no call to Kubernetes.
func (o *options) observer(log *slog.Logger) (*check.Observer, error) {
	strata := check.InDir{Runner: runner.Runner{Binary: o.strata, Timeout: processTimeout}}
	obs := check.New(o.Config, strata, log)
	if !o.readPods {
		return obs, nil
	}
	cfg, err := kube.InCluster(os.Getenv, o.kubeDir)
	if err != nil {
		return nil, errors.New("--read-pods needs the variables KUBERNETES_SERVICE_HOST and KUBERNETES_SERVICE_PORT")
	}
	return obs.WithPods(kube.New(cfg)), nil
}

// alerting lets the observer send the alerts of the file, when the flag names one. It reads the address of
// each channel first, so that a wrong Secret stops the start and not the first alert.
func (o *options) alerting(obs *check.Observer, log *slog.Logger) error {
	if o.rules == nil {
		return nil
	}
	hook := alert.NewWebhook(o.rules.Channels, o.allowHTTP)
	if err := hook.Check(); err != nil {
		return err
	}
	obs.WithAlerts(alert.New(*o.rules, hook, o.digestEvery, log))
	return nil
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
	obs, err := o.observer(log)
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 2
	}
	summary, err := obs.Once(ctx)
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
	obs, err := o.observer(log)
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer:", err)
		return 2
	}
	if err := o.alerting(obs, log); err != nil {
		fmt.Fprintln(stderr, "strata-observer: --alerts:", err)
		return 2
	}
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

// seedRecords writes the first-seen records of the cluster into the Secrets of the records, unless a
// Secret already holds a record. The job of an install runs it once, with a role that can get and
// update the four Secrets and do nothing else. It exits 0 when it wrote the records or kept the ones
// there, and 2 on an error, with the kind of the error and no message of Palette or of Kubernetes.
func seedRecords(ctx context.Context, args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("seed", flag.ContinueOnError)
	fs.SetOutput(stderr)
	clusterUID := fs.String("cluster-uid", os.Getenv("STRATA_CLUSTER_UID"), "the uid of the cluster in Palette")
	namespace := fs.String("namespace", "", "the namespace of the Secrets; the default is the namespace of the pod")
	work := fs.String("work", os.TempDir(), "a directory for the files of strata")
	strataBin := fs.String("strata", "/usr/local/bin/strata", "the strata binary")
	kubeDir := fs.String("kube-dir", kube.ServiceDir, "the directory of the token, the CA, and the namespace of the pod")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if *clusterUID == "" {
		fmt.Fprintln(stderr, "strata-observer seed: --cluster-uid is empty")
		return 2
	}
	if *namespace == "" {
		raw, err := os.ReadFile(filepath.Join(*kubeDir, "namespace"))
		if err != nil || strings.TrimSpace(string(raw)) == "" {
			fmt.Fprintln(stderr, "strata-observer seed: give --namespace: the pod has no namespace file")
			return 2
		}
		*namespace = strings.TrimSpace(string(raw))
	}
	cfg, err := kube.InCluster(os.Getenv, *kubeDir)
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer seed:", err)
		return 2
	}
	strata := check.InDir{Runner: runner.Runner{Binary: *strataBin, Timeout: processTimeout}}
	res, err := seed.Run(ctx, strata, kube.New(cfg), *namespace, *clusterUID, *work)
	if err != nil {
		fmt.Fprintln(stderr, "strata-observer seed:", err)
		return 2
	}
	if res.Kept {
		fmt.Fprintln(stdout, "strata-observer seed: the Secrets hold records, so the seed wrote nothing")
		return 0
	}
	fmt.Fprintf(stdout, "strata-observer seed: wrote %d first-seen records\n", res.Records)
	return 0
}
