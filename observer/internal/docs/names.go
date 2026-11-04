package docs

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
// observer knows. A kind never carries a message.
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
