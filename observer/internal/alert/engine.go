package alert

import (
	"context"
	"log/slog"
	"maps"
	"slices"
	"strings"
	"time"
)

// Known says which news a cycle read. An item whose news is known and whose subject is gone has
// cleared. A failed read clears nothing, so Content is false after a failed content cycle.
type Known struct {
	// Content says that a content cycle ended well: it knows the findings and the loss of contact.
	Content bool
	// Image says that the observer reads the pods and that the cycle ended well. A cycle that reads no
	// pod finds no finding of an image, and that proves nothing. A cycle whose read of the pods failed
	// sets Image too, and the ids in held keep the findings of an image.
	Image bool
	// Newer says that a versions cycle has ended well once: it knows the newer versions.
	Newer bool
}

func (k Known) has(n News) bool {
	switch n {
	case NewsNewer:
		return k.Newer
	case NewsImage:
		return k.Image
	}
	return k.Content
}

// Engine decides which messages to send, from the rules and the store, and sends them.
type Engine struct {
	rules  Rules
	sender Sender
	every  time.Duration
	log    *slog.Logger
}

// New makes an engine. every is the period of a digest.
func New(rules Rules, sender Sender, every time.Duration, log *slog.Logger) *Engine {
	return &Engine{rules: rules, sender: sender, every: every, log: log}
}

// Plan reads what is open now and queues the messages that follow. A subject that opens queues one
// message, and a subject that was open and is gone queues one more, when its news is known. The ids in
// held are subjects that a read could not settle, so they stay as they are. cluster names the cluster
// in the messages. Plan sends nothing: Deliver does.
func (e *Engine) Plan(st *Store, subjects []Subject, known Known, held map[string]bool, cluster string, now time.Time) {
	st.Fill()
	st.forget(e.rules.Channels)
	open := make(map[string]bool, len(subjects))
	for _, s := range subjects {
		open[s.ID] = true
		e.route(st, s, cluster, now)
	}
	for _, id := range slices.Sorted(maps.Keys(st.Items)) {
		if it := st.Items[id]; !open[id] && !held[id] && known.has(it.News) {
			e.clear(st, it, cluster, now)
		}
	}
	e.digest(st, cluster, now)
}

// route follows the rule that decides a subject. A mute keeps the item as it is, and a message of the
// item that waits for a retry goes no more. With no rule, the subject has no alert, and the store
// forgets it.
func (e *Engine) route(st *Store, s Subject, cluster string, now time.Time) {
	i, ok := e.rules.decide(s, now)
	if !ok {
		delete(st.Items, s.ID)
		st.cancel(s.ID)
		return
	}
	r := e.rules.List[i]
	if r.mute() {
		// a message that waits for a retry goes no more: the item stays, and it queues again when the mute ends
		st.cancel(s.ID)
		return
	}
	it, found := st.Items[s.ID]
	if !found {
		it = Item{To: r.To}
	}
	// the item keeps what the subject says now: a digest and a clear tell the latest
	it.Subject = s
	st.Items[s.ID] = it
	if r.Send == SendNow && it.Sent == nil && !st.queued(s.ID) {
		st.queue(Queued{Channel: r.To, Text: openText(cluster, s), Opens: []string{s.ID}})
	}
}

// clear removes an item whose subject is gone. It queues the message of the clear when the open message
// went, the channel of that message is still in the rules, and the rule still sends at once. An open
// message that did not go is dropped: it would announce what is gone.
func (e *Engine) clear(st *Store, it Item, cluster string, now time.Time) {
	delete(st.Items, it.ID)
	if it.Sent == nil {
		st.cancel(it.ID)
		return
	}
	if _, known := e.rules.Channels[it.To]; !known {
		return
	}
	if i, ok := e.rules.decide(it.Subject, now); ok && e.rules.List[i].Send == SendNow {
		st.queue(Queued{Channel: it.To, Text: clearText(cluster, it.Subject, now)})
	}
}

// digest queues one message for each channel that has an open item under a digest rule, when a period
// has passed since the last digest. A digest that did not go is made again from the items that are open
// now, for its channel only, so a channel that comes back gets one digest and no old one, and a channel
// that got its digest gets no second one. With nothing to list, it waits, so the first item to appear
// goes at the next content cycle.
func (e *Engine) digest(st *Store, cluster string, now time.Time) {
	waiting := st.dropDigests()
	due := st.DigestAt == nil || now.Sub(*st.DigestAt) >= e.every
	if !due && len(waiting) == 0 {
		return
	}
	byChannel := map[string][]Item{}
	for _, id := range slices.Sorted(maps.Keys(st.Items)) {
		it := st.Items[id]
		if i, ok := e.rules.decide(it.Subject, now); ok && e.rules.List[i].Send == SendDigest {
			to := e.rules.List[i].To
			byChannel[to] = append(byChannel[to], it)
		}
	}
	if len(byChannel) == 0 {
		return
	}
	for _, channel := range slices.Sorted(maps.Keys(byChannel)) {
		if !due && !waiting[channel] {
			continue
		}
		items := byChannel[channel]
		slices.SortStableFunc(items, func(a, b Item) int {
			return strings.Compare(a.Profile+a.Detail+a.ID, b.Profile+b.Detail+b.ID)
		})
		subjects := make([]Subject, 0, len(items))
		ids := make([]string, 0, len(items))
		for _, it := range items {
			subjects, ids = append(subjects, it.Subject), append(ids, it.ID)
		}
		st.queue(Queued{Channel: channel, Text: digestText(cluster, subjects), Opens: ids, Digest: true})
	}
	if due {
		at := now
		st.DigestAt = &at
	}
}

// Deliver sends the messages of the queue in order and says for each one whether it went. A channel
// that fails keeps its later messages for the next try, so the order of a channel holds. The log has the
// name of the channel and the status, and never the address or the text.
func (e *Engine) Deliver(ctx context.Context, queue []Queued) []bool {
	done := make([]bool, len(queue))
	down := map[string]bool{}
	for i, q := range queue {
		if down[q.Channel] || ctx.Err() != nil {
			continue
		}
		err := e.sender.Send(ctx, q.Channel, q.Text)
		if err != nil {
			down[q.Channel] = true
			e.log.Warn("alert", append([]any{"channel", q.Channel, "verdict", "failed"}, faultFields(err)...)...)
			continue
		}
		done[i] = true
		e.log.Info("alert", "channel", q.Channel, "verdict", "sent")
	}
	return done
}
