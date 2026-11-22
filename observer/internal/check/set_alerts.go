package check

import (
	"context"
	"fmt"
	"slices"

	"github.com/como-technologies/strata/observer/internal/alert"
)

// alertsOfSet hands one content cycle of the set to the alerts. good says that the cycle ended well. A member whose
// check failed clears nothing: each of its subjects stays as it is, as the ids in c.held do. The step plans under the
// lock and sends outside it, as the observer of one cluster does: see Observer.alertStep.
func (s *Set) alertsOfSet(ctx context.Context, good bool, c setCycle) {
	if s.alerts == nil || ctx.Err() != nil {
		return
	}
	subjects, held := s.subjectsOfSet(c)
	s.mu.Lock()
	known := alert.Known{Content: good, Image: good && s.pods != nil, Newer: s.versionsAt != nil}
	s.alerts.Plan(&s.state.Alerts, subjects, known, held, s.label(), s.now())
	queue := slices.Clone(s.state.Alerts.Outbox)
	s.save()
	s.mu.Unlock()
	done := s.alerts.Deliver(ctx, queue)
	s.mu.Lock()
	defer s.mu.Unlock()
	s.state.Alerts.Settle(queue, done, s.now())
	s.save()
}

// subjectsOfSet lists what is open in each cluster of the set, and the loss of contact of the set. It gives the ids
// that the cycle c cannot clear: the ids of c.held, and each subject of a member whose check failed.
func (s *Set) subjectsOfSet(c setCycle) ([]alert.Subject, map[string]bool) {
	held := map[string]bool{}
	for id := range c.held {
		held[id] = true
	}
	var out []alert.Subject
	for _, m := range s.memberList() {
		subjects := alert.MemberSubjects(m.obs.Summary())
		for _, sub := range subjects {
			if c.failed[m.uid] {
				held[sub.ID] = true
			}
		}
		out = append(out, subjects...)
	}
	if lost, ok := alert.SetLost(s.Clusters()); ok {
		out = append(out, lost)
	}
	return out, held
}

// label names the set in a message that is about no one cluster: a digest, and the sample of the page.
func (s *Set) label() string {
	s.mu.Lock()
	n := len(s.members)
	s.mu.Unlock()
	return fmt.Sprintf("%d clusters", n)
}

// Alerts gives the rules with their states for the subjects of each cluster of the set, and says so when the
// observer sends no alert.
func (s *Set) Alerts() alert.View {
	if s.alerts == nil {
		return alert.Unset()
	}
	subjects, _ := s.subjectsOfSet(setCycle{})
	label := s.label()
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.alerts.View(s.state.Alerts, subjects, label, s.now())
}

// Alerts gives the alerts of the set: a cluster has no alerts of its own.
func (v *MemberView) Alerts() alert.View {
	return v.set.Alerts()
}
