package alert

import (
	"slices"
	"time"
)

// maxQueued is the most messages that the store keeps for a retry. A channel that stays down for days
// must not fill the volume of the state. The oldest message goes first, and its subject asks again.
const maxQueued = 100

// Store is what the observer keeps of its alerts between two runs: the open subjects, the messages that
// did not go, and the time of the last digest. It holds names, times, and fixed words, and no value.
type Store struct {
	Items    map[string]Item `json:"items"`
	Outbox   []Queued        `json:"outbox"`
	DigestAt *time.Time      `json:"digestAt,omitempty"`
}

// Item is a subject that is open. To is the channel that a message of this item goes to. Sent is the
// time that its message went: the open message, or the digest that listed it.
type Item struct {
	Subject
	To   string     `json:"to"`
	Sent *time.Time `json:"sent,omitempty"`
}

// Queued is a message that did not go yet. Opens holds the ids of the items that become sent when the
// message goes. A digest lists items, so the next step makes it again from the items that are open then.
type Queued struct {
	Channel string   `json:"channel"`
	Text    string   `json:"text"`
	Opens   []string `json:"opens,omitempty"`
	Digest  bool     `json:"digest,omitempty"`
}

// NewStore makes an empty store.
func NewStore() Store {
	return Store{Items: map[string]Item{}, Outbox: []Queued{}}
}

// Fill gives a map or a list that a file of the state did not hold.
func (st *Store) Fill() {
	if st.Items == nil {
		st.Items = map[string]Item{}
	}
	if st.Outbox == nil {
		st.Outbox = []Queued{}
	}
}

// queued says that a message that opens the item waits in the outbox.
func (st *Store) queued(id string) bool {
	return slices.ContainsFunc(st.Outbox, func(q Queued) bool { return slices.Contains(q.Opens, id) })
}

func (st *Store) queue(q Queued) {
	st.Outbox = append(st.Outbox, q)
	if over := len(st.Outbox) - maxQueued; over > 0 {
		st.Outbox = slices.Clone(st.Outbox[over:])
	}
}

// cancel drops the message that only opens this item. The item cleared before its message went, so the
// message would announce what is gone.
func (st *Store) cancel(id string) {
	st.Outbox = slices.DeleteFunc(st.Outbox, func(q Queued) bool { return len(q.Opens) == 1 && q.Opens[0] == id })
}

// dropDigests removes the digests that wait, and says whether it removed one. A digest that waits
// lists items that can be gone by now.
func (st *Store) dropDigests() bool {
	n := len(st.Outbox)
	st.Outbox = slices.DeleteFunc(st.Outbox, func(q Queued) bool { return q.Digest })
	return len(st.Outbox) < n
}

// forget drops the messages for a channel that the rules do not hold now.
func (st *Store) forget(channels map[string]Channel) {
	st.Outbox = slices.DeleteFunc(st.Outbox, func(q Queued) bool { _, ok := channels[q.Channel]; return !ok })
}

// Settle takes the result of Deliver for the messages of queue. A message that went leaves the outbox,
// and the items that it opens become sent at now. A message that did not go stays.
func (st *Store) Settle(queue []Queued, done []bool, now time.Time) {
	kept := []Queued{}
	for i, q := range queue {
		if !done[i] {
			kept = append(kept, q)
			continue
		}
		for _, id := range q.Opens {
			if it, ok := st.Items[id]; ok {
				sent := now
				it.Sent = &sent
				st.Items[id] = it
			}
		}
	}
	st.Outbox = kept
}
