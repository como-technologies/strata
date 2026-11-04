package docs

// SnapshotSchema is the schema id of a record that the observer reads.
const SnapshotSchema = "strata-snapshot/1"

// The kinds of an error that the observer names. The first four come from strata. The others come
// from the runner, which cannot read a kind from a process that wrote a plain line. A kind never
// carries a message.
const (
	KindAPI       = "api"
	KindTransport = "transport"
	KindNotJSON   = "not-json"
	KindIO        = "io"
	KindTimeout   = "timeout"
	KindNoProcess = "no-process"
	KindCrash     = "crash"
	KindUnknown   = "unknown"
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

// Paths lists the paths of the changed values of a pack. It drops the old and the new value.
func (v Values) Paths() []string {
	out := make([]string, 0, len(v.Diff))
	for _, l := range v.Diff {
		out = append(out, l.Path)
	}
	return out
}

// PackHelm is the type of a pack that holds a Helm chart.
const PackHelm = "helm"
