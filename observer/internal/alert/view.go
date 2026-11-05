package alert

import (
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
)

// State is what a rule shows on the page, in the words of a Prometheus user. Firing says that a subject is
// open and its message went. Pending says that a subject waits for its digest or for a retry. Inactive
// says that no subject is open. Muted says that a mute holds.
type State string

// The states of a rule.
const (
	StateFiring   State = "firing"
	StatePending  State = "pending"
	StateInactive State = "inactive"
	StateMuted    State = "muted"
)

// View is the answer of the route /v1/alerts. It names the channels and never holds an address.
type View struct {
	// Set says that the observer started with --alerts. A view that is not set has no rule.
	Set         bool       `json:"set"`
	Channels    []string   `json:"channels"`
	DigestEvery string     `json:"digestEvery"`
	Waiting     int        `json:"waiting"`
	Rules       []RuleView `json:"rules"`
	Sample      string     `json:"sample"`
}

// RuleView is one rule with its state.
type RuleView struct {
	Level     Level  `json:"level"`
	Match     string `json:"match,omitempty"`
	News      News   `json:"news"`
	Send      Send   `json:"send,omitempty"`
	To        string `json:"to,omitempty"`
	MuteUntil string `json:"muteUntil,omitempty"`
	State     State  `json:"state"`
}

// Unset is the view of an observer that started without --alerts.
func Unset() View {
	return View{Channels: []string{}, Rules: []RuleView{}}
}

// View shows the rules, each with its state at now, for the subjects that are open.
func (e *Engine) View(st Store, subjects []Subject, cluster string, now time.Time) View {
	v := View{Set: true, Channels: e.rules.Names(), DigestEvery: model.ShortDuration(e.every), Waiting: len(st.Outbox), Rules: make([]RuleView, 0, len(e.rules.List)),
		Sample: Sample(cluster)}
	states := make([]State, len(e.rules.List))
	for i, r := range e.rules.List {
		states[i] = StateInactive
		if r.mute() && r.live(now) {
			states[i] = StateMuted
		}
	}
	for _, s := range subjects {
		i, ok := e.rules.decide(s, now)
		if !ok || e.rules.List[i].mute() {
			continue
		}
		if it, found := st.Items[s.ID]; found && it.Sent != nil {
			states[i] = StateFiring
		} else if states[i] != StateFiring {
			states[i] = StatePending
		}
	}
	for i, r := range e.rules.List {
		v.Rules = append(v.Rules, RuleView{Level: r.Level, Match: r.Match, News: r.News, Send: r.Send, To: r.To, MuteUntil: r.MuteUntil, State: states[i]})
	}
	return v
}
