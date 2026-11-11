package docs

import (
	"regexp"
	"strconv"
)

// The schema ids of the documents that the observer reads. A document with another id is not the
// document that the command promises: `--strata` can name another binary. SnapshotSchema is the id of a
// record, and of the snapshot of a cluster.
const (
	SnapshotSchema      = "strata-snapshot/1"
	VerifySummarySchema = "strata-verify-summary/1"
	DiffSchema          = "strata-diff/1"
	BehindSchema        = "strata-behind/1"
)

// The kinds of an error that the observer names. The first four come from strata. The others come
// from the runner, which cannot read a kind from a process that wrote a plain line, and from the
// check of a document: KindSchema says that the schema id of a document is not the one that the
// observer knows. The three kinds that start with kube come from the read of the pods: the API
// server answered with a status, no request got an answer, or the credentials are not usable. A
// kind never carries a message.
const (
	KindAPI       = "api"
	KindTransport = "transport"
	KindNotJSON   = "not-json"
	KindIO        = "io"
	KindTimeout   = "timeout"
	KindNoProcess = "no-process"
	KindCrash     = "crash"
	KindUnknown   = "unknown"
	KindSchema    = "schema"

	KindKubeAPI       = "kube-api"
	KindKubeTransport = "kube-transport"
	KindKubeConfig    = "kube-config"
)

// The statuses of a row of strata-verify-summary/1.
const (
	VerifyUnchanged  = "unchanged"
	VerifyDrifted    = "drifted"
	VerifyUnverified = "unverified"
	VerifyError      = "error"
)

// The statuses of a pack in strata-diff/1.
const (
	PackUnchanged = "unchanged"
	PackChanged   = "changed"
	PackAdded     = "added"
	PackRemoved   = "removed"
)

// The flags that a pack change can carry, as strata-behind/1 names them.
const (
	FlagIdentity = "IDENTITY"
	FlagUpstream = "UPSTREAM"
	FlagAuthored = "AUTHORED"
	FlagImages   = "IMAGES"
	FlagCharts   = "CHARTS"
)

// Flags lists the flags of a pack change, in the order that strata uses.
func (p Pack) Flags() []string {
	out := []string{}
	for _, f := range []struct {
		on   bool
		name string
	}{{p.Identity, FlagIdentity}, {p.Upstream, FlagUpstream}, {p.Authored, FlagAuthored}, {p.Images, FlagImages}, {p.Charts, FlagCharts}} {
		if f.on {
			out = append(out, f.name)
		}
	}
	return out
}

// Flags lists each flag that a pack of the diff carries, in the order that strata uses. These are the
// flags of the move of a profile, as `behind` names them.
func (d Diff) Flags() []string {
	var all Pack
	for _, p := range d.Packs {
		all.Identity, all.Upstream, all.Authored = all.Identity || p.Identity, all.Upstream || p.Upstream, all.Authored || p.Authored
		all.Images, all.Charts = all.Images || p.Images, all.Charts || p.Charts
	}
	return all.Flags()
}

// PackHelm is the type of a pack that holds a Helm chart.
const PackHelm = "helm"

// Document is a document that strata writes. The observer reads it only when SchemaID is the id that
// the command promises.
type Document interface {
	// SchemaID gives the id that the document states.
	SchemaID() string
}

// SchemaID gives the id that the document states.
func (d VerifySummary) SchemaID() string { return d.Schema }

// SchemaID gives the id that the document states.
func (d Diff) SchemaID() string { return d.Schema }

// SchemaID gives the id that the document states.
func (d Behind) SchemaID() string { return d.Schema }

// SchemaID gives the id that the document states.
func (d ClusterRead) SchemaID() string { return d.Schema }

// statusPrefix matches the start of a reason that strata gives for a request that failed:
// `HTTP <status>: <body>`. The other reasons are the message of a request that did not complete.
var statusPrefix = regexp.MustCompile(`^HTTP ([0-9]{3})(:|$)`)

// StatusOf gives the HTTP status that a reason of strata starts with, or 0 when it holds none. The
// body of an answer of Palette follows the status in the reason, and it never leaves this function.
func StatusOf(reason string) int {
	m := statusPrefix.FindStringSubmatch(reason)
	if m == nil {
		return 0
	}
	status, err := strconv.Atoi(m[1])
	if err != nil {
		return 0
	}
	return status
}
