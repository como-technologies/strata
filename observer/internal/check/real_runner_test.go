package check

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// script stands for the strata binary. It prints the smallest document that each command promises.
const script = `#!/bin/sh
cmd=$1
shift
out=
while [ $# -gt 0 ]; do
  if [ "$1" = -o ]; then out=$2; fi
  shift
done
case $cmd in
verify) echo '{"schema":"` + docs.VerifySummarySchema + `","tool":{"version":"0.29.0"},"records":[{"path":"record-0.json","status":"unchanged","name":"aws-stack","version":"1.1.0"}]}' ;;
snapshot) echo '{"schema":"` + docs.SnapshotSchema + `","source":{"cluster":{"uid":"c1","name":"payments","profiles":[{"name":"aws-stack","version":"1.1.0","type":"cluster"}]}}}' > "$out" ;;
compare) echo '{"schema":"` + docs.DiffSchema + `","same":true,"sameContent":true}' ;;
behind) echo '{"schema":"` + docs.BehindSchema + `","tool":{"version":"0.29.0"},"pins":[{"name":"kubernetes","layer":"k8s","registryUid":"reg1","version":"1.35.3","behind":true,"pinnedBy":[{"subject":"aws-stack@1.1.0"}],"newer":[{"version":"1.36.2"}]}]}' ;;
esac
`

func TestTheRunnerAcceptsEveryCommandAndFlagOfACycle(t *testing.T) {
	r := newRig(t)
	r.record("aws-stack", "1.1.0", "k8s/kubernetes")
	bin := filepath.Join(t.TempDir(), "strata")
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	strata := InDir{Runner: runner.Runner{Binary: bin, Timeout: 10 * time.Second}}
	o := newObserver(r.cfg, strata, nil2(), time.Now)
	s, err := o.Once(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if s.Page != model.PageOK || len(s.Newer) != 1 || s.Profiles[0].ClusterCopy.Status != model.CopySame || s.Cluster.Name != "payments" {
		t.Errorf("summary %+v", s)
	}
	if names := entries(t, r.work); len(names) != 0 {
		t.Errorf("work holds %v", names)
	}
}
