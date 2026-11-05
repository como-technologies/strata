package check

import (
	"context"
	"errors"
	"slices"
	"strings"

	"github.com/como-technologies/strata/observer/internal/baseline"
	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
	"github.com/como-technologies/strata/observer/internal/runner"
)

// askKey names one question: the profile, and the cache key of its move. The key holds the version of
// the binary and the move, so a pin that moves to a newer version is another question.
type askKey struct {
	subject, move string
}

// Ask asks Palette whether it gives an error for the move of one pack of a profile to the newest
// version. It runs `behind --pin` on the record of the profile, which sends one POST for each record
// that pins the pack, and no other request that changes anything. A person asks by a click: nothing
// calls Ask on a timer.
//
// An answer stays in memory until the pin moves or the process restarts. A second call for an answered
// move sends nothing. A request that failed is not an answer, so the next call asks again. The second
// result is false when the profile, the pack, or the move is not known. The call waits for a cycle that
// holds the worker, and it ends with the context. A versions cycle can run during the wait and move
// the pin. So the call reads the pin again when it has the worker, and the answer belongs to the move
// that the page shows.
func (o *Observer) Ask(ctx context.Context, subject, key string) (model.Answer, bool, error) {
	pin, move, ok := o.pinOf(subject, key)
	if !ok {
		return model.Answer{}, false, nil
	}
	k := askKey{subject, move}
	if a, ok := o.cachedAsk(k); ok && a.Answered() {
		return a, true, nil
	}
	if err := o.acquire(ctx); err != nil {
		return model.Answer{}, false, err
	}
	defer o.release()
	// No cycle runs while this call holds the worker, so the pin that it reads now holds until the end.
	if pin, move, ok = o.pinOf(subject, key); !ok {
		return model.Answer{}, false, nil
	}
	// A request that waited for the worker can find the answer that another request got.
	k = askKey{subject, move}
	if a, ok := o.cachedAsk(k); ok && a.Answered() {
		return a, true, nil
	}
	answer, found, err := o.askPalette(ctx, subject, key, pin)
	if err != nil || !found {
		return model.Answer{}, false, err
	}
	o.storeAsk(k, answer)
	o.log.Info("ask", "verdict", answer.Verdict, "status", answer.Status)
	return answer, true, nil
}

// askPalette runs `behind --pin` on the record of the profile. A run that fails, or a document with no
// answer of Palette, is a verdict of no answer with the status of the failure. Only a stop of ctx, and a
// directory that the observer cannot make, are an error. The second result is false when no record
// holds the profile.
func (o *Observer) askPalette(ctx context.Context, subject, key string, pin docs.Pin) (model.Answer, bool, error) {
	set, err := baseline.Load(o.cfg.Records)
	if err != nil {
		return model.Answer{}, false, failure(docs.KindIO)
	}
	i := slices.IndexFunc(set.Entries, func(e baseline.Entry) bool {
		return model.Subject(e.Record.Source.Name, e.Record.Source.Version) == subject
	})
	if i < 0 {
		return model.Answer{}, false, nil
	}
	answer := model.Answer{Profile: subject, Key: key, Newest: pin.Newer[0].Version, Verdict: model.VerdictNoAnswer, Refusals: []docs.Refusal{}}
	err = o.withCycleDir(func(dir string) error {
		names, err := writeRecords(dir, set.Entries[i:i+1])
		if err != nil {
			return err
		}
		res, err := o.run(ctx, dir, exitsWithDocument, runner.CmdBehind, names[0], runner.FlagPin, key, "--json", "--compact", "-q")
		if err == nil {
			var doc docs.Behind
			if doc, err = decode[docs.Behind](res.Stdout, docs.BehindSchema); err == nil {
				answer = model.AnswerOf(subject, key, askedPin(doc, pin))
			}
		}
		return o.failedAsk(ctx, err, &answer)
	})
	return answer, err == nil, err
}

// askedPin finds the pin that the question is about in the document of `behind`. The document holds
// the pins of the pack in the record, and the record pins it one time. A document without the pin gives
// the pin of the cycle, which has no bump, so the answer is no answer.
//
// The answer is for the move of the pin of the cycle, which the page shows. A pin that failed, or that
// is not behind any more, has no newer version in the document. A registry can also stop to list the
// newest version of the cycle. In these cases the pin gets the newer versions of the cycle and no bump.
// So the answer names the version of the page, and it says no answer. A registry that gained a version
// lists the version of the cycle still, and the answer is for the newer version.
func askedPin(doc docs.Behind, asked docs.Pin) docs.Pin {
	for _, p := range doc.Pins {
		if p.Name == asked.Name && p.Version == asked.Version && p.RegistryUID == asked.RegistryUID {
			if !slices.ContainsFunc(p.Newer, func(n docs.Newer) bool { return n.Version == asked.Newer[0].Version }) {
				p.Newer, p.Bump = asked.Newer, nil
			}
			return p
		}
	}
	asked.Bump, asked.Error = nil, nil
	return asked
}

// failedAsk turns the failure of a run into the answer "no answer", with the status of the run. A stop
// of ctx stays an error, because the person is gone and no answer is due.
func (o *Observer) failedAsk(ctx context.Context, err error, answer *model.Answer) error {
	if err == nil {
		return nil
	}
	var re *runner.RunError
	if ctx.Err() != nil || !errors.As(err, &re) || re.Kind == docs.KindIO {
		return err
	}
	answer.Verdict, answer.Status = model.VerdictNoAnswer, re.Status
	return nil
}

// Answers gives the answer of each question that Palette answered or failed to answer since the start,
// sorted by profile and pack. Only the detail tier serves it.
func (o *Observer) Answers() []model.Answer {
	o.mu.Lock()
	defer o.mu.Unlock()
	out := make([]model.Answer, 0, len(o.asks))
	for _, a := range o.asks {
		out = append(out, a)
	}
	slices.SortFunc(out, func(a, b model.Answer) int {
		return strings.Compare(a.Profile+" "+a.Key, b.Profile+" "+b.Key)
	})
	return out
}

func (o *Observer) cachedAsk(k askKey) (model.Answer, bool) {
	o.mu.Lock()
	defer o.mu.Unlock()
	a, ok := o.asks[k]
	return a, ok
}

func (o *Observer) storeAsk(k askKey, a model.Answer) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.asks[k] = a
}

// maxText is the most unified text that the observer keeps for one pack on one side of a diff: 256 KiB
// for the values and the manifests together. The pod has a memory limit of 512 Mi, and a pack above
// the cap shows no text and names the command for it.
const maxText = 256 << 10
