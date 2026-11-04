package check

import (
	"context"
	"log/slog"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// script stands for the strata binary. It prints the smallest document that each command promises.
func script(t *testing.T) string {
	t.Helper()
	return testkit.Strata(t, `case $cmd in
verify) echo '`+testkit.VerifyUnchanged+`' ;;
snapshot) echo '`+testkit.ClusterPayments+`' > "$out" ;;
compare) echo '`+testkit.CompareSame+`' ;;
behind) echo '{"schema":"`+docs.BehindSchema+`","tool":{"version":"0.29.0"},"pins":[{"name":"kubernetes","layer":"k8s","registryUid":"reg1","version":"1.35.3","behind":true,"pinnedBy":[{"subject":"aws-stack@1.1.0"}],"newer":[{"version":"1.36.2"}]}]}' ;;
esac`)
}

func TestTheRunnerAcceptsEveryCommandAndFlagOfACycle(t *testing.T) {
	r := newRig(t)
	r.record("aws-stack", "1.1.0", "k8s/kubernetes")
	strata := InDir{Runner: runner.Runner{Binary: script(t), Timeout: 10 * time.Second}}
	o := newObserver(r.cfg, strata, slog.New(slog.DiscardHandler), time.Now)
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
