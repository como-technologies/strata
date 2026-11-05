package check

import (
	"context"
	"errors"
	"time"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/kube"
	"github.com/como-technologies/strata/observer/internal/model"
)

// Pods lists the containers of the pods of the cluster. The Client of internal/kube is the Pods of the
// program, and a test gives a fake.
type Pods interface {
	Pods(ctx context.Context) ([]kube.Container, error)
}

// maxFirstDigests is the number of images whose first digest the state keeps. Above it, the state
// drops the images that the last read did not show.
const maxFirstDigests = 2048

// WithPods tells the observer to read the pods at the start of each content cycle. Without it, the
// observer makes no call to Kubernetes. It gives the observer, so that a program can chain it.
func (o *Observer) WithPods(p Pods) *Observer {
	o.pods = p
	return o
}

// readPods lists the pods at the start of a content cycle. A read that fails is not a failed cycle:
// the pods are not the content. The cycle goes on with the cause pods-not-read, and Carry keeps the
// last good read. The log line holds a verdict, a duration, a kind, and a status, and no pod.
func (o *Observer) readPods(ctx context.Context, content *model.Content) {
	if o.pods == nil {
		return
	}
	start := time.Now()
	containers, err := o.pods.Pods(ctx)
	line := []any{"verdict", "ok", "duration", time.Since(start).Round(time.Millisecond)}
	if err != nil {
		ref := podsError(err)
		content.Pods.Err = &ref
		line = []any{"verdict", "failed", "kind", ref.Kind, "status", ref.Status, "duration", time.Since(start).Round(time.Millisecond)}
	} else {
		content.Pods = model.PodRead{Containers: containers, At: o.now()}
	}
	o.log.Info("pods", line...)
}

// podsError names a failed read of the pods by its kind and status.
func podsError(err error) model.ErrorRef {
	var ke *kube.Error
	if errors.As(err, &ke) {
		return model.ErrorRef{Kind: ke.Kind, Status: ke.Status}
	}
	return model.ErrorRef{Kind: docs.KindUnknown}
}

// keepFirstDigests keeps the first digest of each image that the pods run. A new revision of the
// records drops them all, because the team approved that content. A read that failed adds none: the
// containers of an earlier read are already in the state.
func (o *Observer) keepFirstDigests(content model.Content) {
	if o.pods == nil {
		return
	}
	images := &o.state.Images
	if images.Revision != content.Meta.Revision || images.First == nil {
		*images = ImagesState{Revision: content.Meta.Revision, First: map[string]string{}}
	}
	if content.Pods.Err != nil {
		return
	}
	for id, digest := range model.FirstDigests(content.Pods.Containers, images.First) {
		images.First[id] = digest
	}
	if len(images.First) <= maxFirstDigests {
		return
	}
	seen := map[string]bool{}
	for _, c := range content.Pods.Containers {
		seen[model.ImageKey(c.Image)] = true
	}
	for id := range images.First {
		if !seen[id] {
			delete(images.First, id)
		}
	}
}

// PodsFunc makes a Pods of a function, as http.HandlerFunc makes a Handler. A test gives one.
type PodsFunc func(ctx context.Context) ([]kube.Container, error)

// Pods calls the function.
func (f PodsFunc) Pods(ctx context.Context) ([]kube.Container, error) {
	return f(ctx)
}
