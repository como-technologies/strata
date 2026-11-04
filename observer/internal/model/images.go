package model

import (
	"cmp"
	"slices"
	"strings"
	"time"

	"github.com/como-technologies/strata/observer/internal/kube"
)

// ObservedSchema is the id of the document that the image routes serve. It is no tier of a record and
// it moves no root. It holds no pod name, no namespace, and no environment.
const ObservedSchema = "strata-observed/1"

// PodsState says whether the observer reads pods, and whether a read has ended well.
type PodsState string

// The states of the read of the pods. A read that fails after a good one keeps the state read. The
// document then holds the failure in Error and the data of the last good read.
const (
	PodsOff    PodsState = "off"
	PodsUnread PodsState = "not-read"
	PodsRead   PodsState = "read"
)

// Match says how the image of a pod matched a pack. Exact is the same text. NameAndTag is the same
// name and tag after the prefix docker.io is removed. The page labels it "matched by name and tag".
type Match string

// The matches of an image.
const (
	MatchExact      Match = "exact"
	MatchNameAndTag Match = "name-and-tag"
	MatchNone       Match = "none"
)

// DigestStatus is what the observer says about one digest of an image.
type DigestStatus string

// The statuses of a digest. A pod that never started has no image id, and a local image has an id with
// no repository digest. Neither is checked, and neither reads OK.
const (
	FirstDigest   DigestStatus = "first-digest"
	AnotherDigest DigestStatus = "another-digest"
	NoRepoDigest  DigestStatus = "no-repo-digest"
	NotStarted    DigestStatus = "not-started"
)

// digestOrder is the order of the digests of a row.
var digestOrder = []DigestStatus{FirstDigest, AnotherDigest, NoRepoDigest, NotStarted}

// ImageStatus is the state of one image. Split is one tag with two digests at one time. Moved is one
// digest that is not the first one that the observer saw. NotChecked says that no pod gave a digest.
type ImageStatus string

// The statuses of an image, in the order of the precedence.
const (
	ImageStatusSplit      ImageStatus = "split"
	ImageStatusMoved      ImageStatus = "moved"
	ImageStatusSteady     ImageStatus = "steady"
	ImageStatusNotChecked ImageStatus = "not-checked"
)

// imageOrder is the order of the rows of the list.
var imageOrder = []ImageStatus{ImageStatusSplit, ImageStatusMoved, ImageStatusSteady, ImageStatusNotChecked}

// Observed is strata-observed/1: the images that the pods run, with the digest of each. ReadAt is the
// end of the last good read. Error is the last read, when it failed. Pods is off when the observer
// does not read pods.
type Observed struct {
	Schema string      `json:"schema"`
	Pods   PodsState   `json:"pods"`
	ReadAt *time.Time  `json:"readAt"`
	Error  *ErrorRef   `json:"error,omitempty"`
	Counts ImageCounts `json:"counts"`
	Images []Image     `json:"images"`
}

// ImageCounts holds the numbers of the list. Changed counts the images that make a finding.
type ImageCounts struct {
	Images  int `json:"images"`
	Matched int `json:"matched"`
	Changed int `json:"changed"`
}

// Image is one image reference of the pods, as the spec of a pod names it. ByDigest says that the
// reference names a digest, so it cannot move. Packs names each pack that the image matches.
type Image struct {
	Ref      string      `json:"ref"`
	Status   ImageStatus `json:"status"`
	Match    Match       `json:"match"`
	ByDigest bool        `json:"byDigest"`
	Packs    []ImagePack `json:"packs"`
	Digests  []Digest    `json:"digests"`
}

// ImagePack names a pack of a profile. Key is <layer>/<name>.
type ImagePack struct {
	Profile string `json:"profile"`
	Key     string `json:"key"`
}

// Digest is one digest of an image, with the number of pods that run it. A digest that is empty
// belongs to a pod that never started, or to a local image.
type Digest struct {
	Digest string       `json:"digest,omitempty"`
	Status DigestStatus `json:"status"`
	Pods   int          `json:"pods"`
}

// ImageView is the answer for one image: the document, with the one row.
type ImageView struct {
	Schema string     `json:"schema"`
	Pods   PodsState  `json:"pods"`
	ReadAt *time.Time `json:"readAt"`
	Error  *ErrorRef  `json:"error,omitempty"`
	Image  Image      `json:"image"`
}

// PodRead is what the read of the pods gave. Containers and At come from the last good read: a
// failed read keeps them, see Content.Carry. Err is the failure of the newest read, and nil when
// that read worked.
type PodRead struct {
	Containers []kube.Container
	At         time.Time
	Err        *ErrorRef
}

// read says that a read of the pods ended well at some time.
func (p PodRead) read() bool {
	return !p.At.IsZero()
}

// ImageKey is the id of an image reference in the state: the state keeps ids and digests, and never a
// reference.
func ImageKey(ref string) string {
	return shortID("image", ref)
}

// Finding says whether the image makes a finding, and of which kind. A finding needs a pack that the
// image matches, and a reference that names a tag, because a digest cannot move.
func (i Image) Finding() (Kind, bool) {
	if i.Match == MatchNone || i.ByDigest {
		return "", false
	}
	switch i.Status {
	case ImageStatusMoved:
		return ImageMoved, true
	case ImageStatusSplit:
		return ImageSplit, true
	}
	return "", false
}

// namedByDigest says that a reference names a digest.
func namedByDigest(ref string) bool {
	return strings.Contains(ref, "@sha256")
}

// digestOf gives the repository digest of an image id, or "" when it has none. The id of a local image
// is sha256:<id> with no repository, so it has none.
func digestOf(imageID string) string {
	_, digest, ok := strings.Cut(imageID, "@")
	if !ok || !strings.HasPrefix(digest, "sha256:") {
		return ""
	}
	return digest
}

// dockerHubPrefixes are the prefixes that name the same registry as no prefix. The longer one is first.
var dockerHubPrefixes = []string{"docker.io/library/", "docker.io/"}

// nameAndTag gives the name and the tag of a reference. It removes the prefix of Docker Hub and the
// digest, so that nginx:1.27 and docker.io/library/nginx:1.27 are the same.
func nameAndTag(ref string) string {
	for _, p := range dockerHubPrefixes {
		if rest, ok := strings.CutPrefix(ref, p); ok {
			ref = rest
			break
		}
	}
	name, _, _ := strings.Cut(ref, "@")
	return name
}

// packIndex finds the packs of the records that the cluster runs by the images that they name.
type packIndex struct {
	exact, byName map[string][]ImagePack
}

func newPackIndex(c Content) packIndex {
	x := packIndex{exact: map[string][]ImagePack{}, byName: map[string][]ImagePack{}}
	add := func(m map[string][]ImagePack, key string, p ImagePack) {
		if !slices.Contains(m[key], p) {
			m[key] = append(m[key], p)
		}
	}
	for _, r := range c.Records {
		if !r.Cluster.Runs {
			continue
		}
		for _, rp := range r.Doc.Packs {
			p := ImagePack{Profile: r.subject(), Key: rp.Key}
			for _, image := range rp.Images {
				add(x.exact, image, p)
				add(x.byName, nameAndTag(image), p)
			}
		}
	}
	return x
}

// find gives the packs that name the image. The exact text wins over the name and the tag.
func (x packIndex) find(ref string) ([]ImagePack, Match) {
	packs, match := x.exact[ref], MatchExact
	if len(packs) == 0 {
		packs, match = x.byName[nameAndTag(ref)], MatchNameAndTag
	}
	if len(packs) == 0 {
		return []ImagePack{}, MatchNone
	}
	out := slices.Clone(packs)
	slices.SortFunc(out, func(a, b ImagePack) int {
		return cmp.Or(strings.Compare(a.Profile, b.Profile), strings.Compare(a.Key, b.Key))
	})
	return out, match
}

// podSet counts the distinct pods that run one digest.
type podSet map[string]bool

// digestKey groups the containers of an image. An image id that holds a digest has the status "" here,
// and the status comes from the compare with the first digest.
type digestKey struct {
	digest string
	status DigestStatus
}

func keyOf(imageID string) digestKey {
	switch digest := digestOf(imageID); {
	case imageID == "":
		return digestKey{status: NotStarted}
	case digest == "":
		return digestKey{status: NoRepoDigest}
	default:
		return digestKey{digest: digest}
	}
}

// groupByImage collects, for each reference, the pods of each digest. The order of the references is
// the order of the first container.
func groupByImage(containers []kube.Container) (refs []string, groups map[string]map[digestKey]podSet) {
	groups = map[string]map[digestKey]podSet{}
	for _, c := range containers {
		if groups[c.Image] == nil {
			groups[c.Image] = map[digestKey]podSet{}
			refs = append(refs, c.Image)
		}
		k := keyOf(c.ImageID)
		if groups[c.Image][k] == nil {
			groups[c.Image][k] = podSet{}
		}
		groups[c.Image][k][c.Pod] = true
	}
	return refs, groups
}

// FirstDigests gives the first digest of each image that the containers run and that known does not
// hold yet. It keys the digests by ImageKey. When an image shows two digests at its first sight, the
// digest of more pods wins, and the smaller text wins a tie.
func FirstDigests(containers []kube.Container, known map[string]string) map[string]string {
	out := map[string]string{}
	refs, groups := groupByImage(containers)
	for _, ref := range refs {
		id := ImageKey(ref)
		if _, ok := known[id]; ok {
			continue
		}
		var best digestKey
		var most int
		for k, pods := range groups[ref] {
			if k.digest == "" {
				continue
			}
			if n := len(pods); n > most || n == most && k.digest < best.digest {
				best, most = k, n
			}
		}
		if best.digest != "" {
			out[id] = best.digest
		}
	}
	return out
}

// imageRows gives one row for each image reference that the pods run, the worst status first. first
// holds the first digest of each image by ImageKey. A digest is the first digest when first holds no
// digest for its image, because the observer saw it first.
func imageRows(c Content, first map[string]string) []Image {
	index := newPackIndex(c)
	refs, groups := groupByImage(c.Pods.Containers)
	rows := make([]Image, 0, len(refs))
	for _, ref := range refs {
		img := Image{Ref: ref, ByDigest: namedByDigest(ref), Digests: []Digest{}}
		img.Packs, img.Match = index.find(ref)
		want := first[ImageKey(ref)]
		var real int
		var other bool
		for k, pods := range groups[ref] {
			d := Digest{Digest: k.digest, Status: k.status, Pods: len(pods)}
			if k.digest != "" {
				real++
				d.Status = FirstDigest
				if want != "" && k.digest != want {
					d.Status, other = AnotherDigest, true
				}
			}
			img.Digests = append(img.Digests, d)
		}
		slices.SortFunc(img.Digests, func(a, b Digest) int {
			return cmp.Or(slices.Index(digestOrder, a.Status)-slices.Index(digestOrder, b.Status), strings.Compare(a.Digest, b.Digest))
		})
		switch {
		case real >= 2:
			img.Status = ImageStatusSplit
		case real == 1 && other:
			img.Status = ImageStatusMoved
		case real == 1:
			img.Status = ImageStatusSteady
		default:
			img.Status = ImageStatusNotChecked
		}
		rows = append(rows, img)
	}
	slices.SortFunc(rows, func(a, b Image) int {
		return cmp.Or(slices.Index(imageOrder, a.Status)-slices.Index(imageOrder, b.Status), strings.Compare(a.Ref, b.Ref))
	})
	return rows
}

// imageFindings gives the findings of the images: one for each kind and profile. It names the packs
// and the images that the finding is about. An image that matches no pack, and an image that a digest
// names, make none.
func imageFindings(c Content, first map[string]string) []Finding {
	type group struct {
		kind    Kind
		profile string
	}
	packs, images := map[group][]string{}, map[group][]string{}
	var order []group
	for _, img := range imageRows(c, first) {
		kind, ok := img.Finding()
		if !ok {
			continue
		}
		for _, p := range img.Packs {
			g := group{kind, p.Profile}
			if _, seen := packs[g]; !seen {
				order = append(order, g)
			}
			if !slices.Contains(packs[g], p.Key) {
				packs[g] = append(packs[g], p.Key)
			}
			if !slices.Contains(images[g], img.Ref) {
				images[g] = append(images[g], img.Ref)
			}
		}
	}
	out := make([]Finding, 0, len(order))
	for _, g := range order {
		slices.Sort(packs[g])
		slices.Sort(images[g])
		f := newFinding(g.kind, g.profile, packs[g], "")
		f.Images = images[g]
		out = append(out, f)
	}
	return out
}

// ObservedOf builds the document of the pods from the last good content. A pod read that is off gives
// the state off, and a read that never ended well gives the state not-read and no image.
func ObservedOf(in Input) Observed {
	o := Observed{Schema: ObservedSchema, Pods: PodsOff, Images: []Image{}}
	if !in.ReadPods {
		return o
	}
	o.Pods = PodsUnread
	if in.Content == nil {
		return o
	}
	pods := in.Content.Pods
	o.Error = pods.Err
	if !pods.read() {
		return o
	}
	o.Pods, o.ReadAt = PodsRead, utcPtr(&pods.At)
	o.Images = imageRows(*in.Content, in.FirstDigests)
	o.Counts.Images = len(o.Images)
	for _, img := range o.Images {
		if img.Match != MatchNone {
			o.Counts.Matched++
		}
		if _, ok := img.Finding(); ok {
			o.Counts.Changed++
		}
	}
	return o
}

// ImageOf gives the view of one image of the document, found by its reference.
func (o Observed) ImageOf(ref string) (ImageView, bool) {
	i := slices.IndexFunc(o.Images, func(img Image) bool { return img.Ref == ref })
	if i < 0 {
		return ImageView{}, false
	}
	return ImageView{Schema: o.Schema, Pods: o.Pods, ReadAt: o.ReadAt, Error: o.Error, Image: o.Images[i]}, true
}
