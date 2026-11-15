package check

import (
	"log/slog"
	"testing"
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
	"github.com/como-technologies/strata/observer/internal/testkit"
)

// script stands for the strata binary. It prints the smallest document that each command promises.
func script(t *testing.T) string {
	t.Helper()
	return testkit.Strata(t, testkit.VerifyFirst+testkit.VerifyAndSnapshot+`compare) echo '`+testkit.CompareSame+`' ;;
behind) echo '`+testkit.KubernetesBehind+`' ;;
esac`)
}

func TestTheRunnerAcceptsEveryCommandAndFlagOfACycle(t *testing.T) {
	r := newRig(t)
	r.record("aws-stack", "1.1.0", testkit.KubernetesKey)
	strata := InDir{Runner: runner.Runner{Binary: script(t), Timeout: 10 * time.Second}}
	o := newObserver(r.cfg, strata, slog.New(slog.DiscardHandler), time.Now)
	s := once(t, o)
	if s.Page != model.PageOK || len(s.Newer) != 1 || s.Profiles[0].ClusterCopy.Status != model.CopySame || s.Cluster.Name != testkit.PaymentsName {
		t.Errorf("summary %+v", s)
	}
	if names := entries(t, r.work); len(names) != 0 {
		t.Errorf("work holds %v", names)
	}
}
