package model

// Kind is the cause of one finding.
type Kind string

// The kinds of a finding. One finding is one cause.
const (
	PublishedChanged Kind = "published-changed"
	Overwritten      Kind = "overwritten"
	Renamed          Kind = "renamed"
	Gone             Kind = "gone"
	ClusterDiffers   Kind = "cluster-differs"
	ImageIDChanged   Kind = "image-id-changed"
)

// Cause is the reason that a check did not cover a profile or a pack.
type Cause string

// The causes of the section "Not checked". A cause never raises a severity.
const (
	NoUpstreamDigest  Cause = "no-upstream-digest"
	OlderRegistryCopy Cause = "older-registry-copy"
	NoRecord          Cause = "no-record"
	ReadError         Cause = "read-error"
	PinError          Cause = "pin-error"
)

// Status is the state of a profile or a pack. The order of the constants is the order of the
// precedence: the first status that holds wins.
type Status string

// The statuses of a profile and of a pack. A pack is never no-record.
const (
	StatusDrift      Status = "drift"
	StatusNewer      Status = "newer"
	StatusNotChecked Status = "not-checked"
	StatusOK         Status = "ok"
	StatusNoRecord   Status = "no-record"
)

// CopyStatus is the answer of the compare of a record and the cluster side.
type CopyStatus string

// The statuses of clusterCopy. Same content is the word of the terminal.
const (
	CopySame             CopyStatus = "same"
	CopySameContent      CopyStatus = "same-content"
	CopyDiffers          CopyStatus = "differs"
	CopyNotOnThisCluster CopyStatus = "not-on-this-cluster"
	CopyNotRead          CopyStatus = "not-read"
)

// Page is the state of the whole page.
type Page string

// The states of the page.
const (
	PageNoRecords Page = "no-records"
	PageLost      Page = "lost"
	PageStale     Page = "stale"
	PageFirstLoad Page = "first-load"
	PageOK        Page = "ok"
)

// The scopes of a profile.
const (
	ScopeTenant  = "tenant"
	ScopeProject = "project"
)

// causeOrder is the order of the rows of "Not checked": the order of the table of the causes.
var causeOrder = []Cause{NoUpstreamDigest, OlderRegistryCopy, NoRecord, ReadError, PinError}
