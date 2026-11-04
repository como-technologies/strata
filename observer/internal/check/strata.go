// Package check runs the two timers of the observer. A content cycle asks whether the records, the
// published versions, and the cluster agree. A versions cycle asks whether a newer version of a pack
// exists. Each cycle runs the strata binary through the Strata interface and keeps the answers.
package check

import (
	"context"

	"github.com/como-technologies/strata/observer/internal/runner"
)

// Strata runs one strata command in a directory. A test gives a fake that returns files.
type Strata interface {
	Run(ctx context.Context, dir string, args ...string) (runner.Result, error)
}

// InDir makes a Strata of a Runner. The directory of the call replaces the directory of the Runner.
type InDir struct {
	Runner runner.Runner
}

// Run starts the strata binary in dir.
func (d InDir) Run(ctx context.Context, dir string, args ...string) (runner.Result, error) {
	r := d.Runner
	r.Dir = dir
	return r.Run(ctx, args...)
}
