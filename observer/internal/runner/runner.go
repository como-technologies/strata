// Package runner runs the strata binary. It is the one place in the observer that starts a process.
package runner

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"slices"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
)

// allowedFlags holds each flag that the observer gives to strata. A flag that asks Palette to
// validate or that prints content in full is not here: slice 1 sends no POST and holds no diff text.
var allowedFlags = []string{"--summary", "--who", "--json", "--compact", "-q", "--no-impact", "--raw-dir", "--from-raw", "-o"}

// The strata commands that the observer runs.
const (
	CmdVerify   = "verify"
	CmdCompare  = "compare"
	CmdSnapshot = "snapshot"
	CmdBehind   = "behind"
)

var allowedCommands = []string{CmdVerify, CmdCompare, CmdSnapshot, CmdBehind}

// allowedEnv holds each variable that a strata process gets. It sets STRATA_REPORT to 0 below.
var allowedEnv = []string{"PALETTE_API", "PALETTE_API_KEY", "PALETTE_PROJECT_UID", "PALETTE_PROJECT", "HOME", "TMPDIR", "XDG_STATE_HOME", "PATH"}

// Runner runs one binary with one time limit for each run.
type Runner struct {
	Binary  string
	Dir     string
	Timeout time.Duration
}

// Result is a run that gave an answer. Exit is 0 or 1, or 2 with a document on stdout.
type Result struct {
	Exit   int
	Stdout []byte
}

// RunError is a run with no answer: exit 2 with an error line, a killed process, or no process. It
// holds a kind and a status only, so a log line or an answer of the API cannot carry the message of
// Palette. A command with no JSON mode, such as snapshot, writes a plain line, and its kind is
// unknown. The kind of a failed cycle then comes from records[].error of verify, and pins[].error
// of behind.
type RunError struct {
	Kind   string
	Status int
}

func (e *RunError) Error() string {
	if e.Status != 0 {
		return fmt.Sprintf("strata: %s, status %d", e.Kind, e.Status)
	}
	return "strata: " + e.Kind
}

// Run starts strata and returns its stdout. It refuses a command or a flag that is not allowed.
func (r Runner) Run(ctx context.Context, args ...string) (Result, error) {
	if err := check(args); err != nil {
		return Result{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, r.Timeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, r.Binary, args...)
	cmd.Dir = r.Dir
	cmd.Env = env()
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	err := cmd.Run()
	if ctx.Err() != nil {
		return Result{}, &RunError{Kind: docs.KindTimeout}
	}
	exit := 0
	if err != nil {
		var ee *exec.ExitError
		if !errors.As(err, &ee) {
			return Result{}, &RunError{Kind: docs.KindNoProcess}
		}
		exit = ee.ExitCode()
	}
	if exit == 2 && stdout.Len() == 0 {
		return Result{}, parseError(stderr.Bytes())
	}
	if exit > 2 || exit < 0 {
		return Result{}, &RunError{Kind: docs.KindCrash}
	}
	return Result{Exit: exit, Stdout: stdout.Bytes()}, nil
}

func check(args []string) error {
	if len(args) == 0 || !slices.Contains(allowedCommands, args[0]) {
		return fmt.Errorf("runner: the command %q is not allowed", args)
	}
	for _, a := range args[1:] {
		if len(a) > 0 && a[0] == '-' && !slices.Contains(allowedFlags, a) {
			return fmt.Errorf("runner: the flag %q is not allowed", a)
		}
	}
	return nil
}

func env() []string {
	out := []string{"STRATA_REPORT=0", "STRATA_QUIET=1", "NO_COLOR=1"}
	for _, k := range allowedEnv {
		if v, ok := os.LookupEnv(k); ok {
			out = append(out, k+"="+v)
		}
	}
	return out
}

func parseError(line []byte) error {
	var doc struct {
		Error docs.Error `json:"error"`
	}
	if err := json.Unmarshal(line, &doc); err != nil || doc.Error.Kind == "" {
		return &RunError{Kind: docs.KindUnknown}
	}
	return &RunError{Kind: doc.Error.Kind, Status: doc.Error.Status}
}
