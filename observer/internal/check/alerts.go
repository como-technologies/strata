package check

import (
	"context"
	"slices"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/model"
)

// WithAlerts tells the observer to send alerts after each content cycle. Without it, the observer
// sends nothing and `once` never sends. It gives the observer, so that a program can chain it.
func (o *Observer) WithAlerts(e *alert.Engine) *Observer {
	o.alerts = e
	return o
}

// alertStep hands the last content cycle to the alerts. good says that the cycle ended well, and held
// holds the findings that this content cannot clear. A cycle that failed only lets the alert "lost"
// open: a failed read clears nothing. The step plans under the lock and sends outside it, so a slow
// webhook never holds a request of the page. The worker runs it, so no two steps run together.
func (o *Observer) alertStep(ctx context.Context, good bool, held map[string]bool) {
	if o.alerts == nil || ctx.Err() != nil {
		return
	}
	o.mu.Lock()
	sum := model.Build(o.input())
	o.alerts.Plan(&o.state.Alerts, alert.SubjectsOf(sum), alert.Known{Content: good, Newer: o.versions != nil}, held, clusterName(sum), o.now())
	queue := slices.Clone(o.state.Alerts.Outbox)
	o.save()
	o.mu.Unlock()
	done := o.alerts.Deliver(ctx, queue)
	o.mu.Lock()
	defer o.mu.Unlock()
	o.state.Alerts.Settle(queue, done, o.now())
	o.save()
}

// clusterName names the cluster in a message: the name that Palette gives, or the uid before a cycle read it.
func clusterName(sum model.Summary) string {
	if sum.Cluster.Name != "" {
		return sum.Cluster.Name
	}
	return sum.Cluster.UID
}

// Alerts gives the rules with their states, and says so when the observer sends no alert.
func (o *Observer) Alerts() alert.View {
	if o.alerts == nil {
		return alert.Unset()
	}
	o.mu.Lock()
	defer o.mu.Unlock()
	sum := model.Build(o.input())
	return o.alerts.View(o.state.Alerts, alert.SubjectsOf(sum), clusterName(sum), o.now())
}
