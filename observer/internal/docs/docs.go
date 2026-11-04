// Package docs holds the fields of the strata documents that the observer can read.
//
// These types are the allow-list. A field that has no line here does not enter the process, so it
// cannot leave it. Do not add a field for a variable, a whole source block, or a whole record.
// Do not decode a document into a map.
package docs

// Tool names the binary that wrote a document.
type Tool struct {
	Version string `json:"version"`
}

// VerifySummary is strata-verify-summary/1.
type VerifySummary struct {
	Schema  string      `json:"schema"`
	Tool    Tool        `json:"tool"`
	Records []VerifyRow `json:"records"`
}

// VerifyRow is one record of a verify. Status is unchanged, drifted, unverified, or error.
type VerifyRow struct {
	Path         string `json:"path"`
	Status       string `json:"status"`
	Name         string `json:"name"`
	Version      string `json:"version"`
	UID          string `json:"uid"`
	RecordedAt   string `json:"recordedAt"`
	RootRecorded string `json:"rootRecorded"`
	RootLive     string `json:"rootLive"`
	MaxImpact    string `json:"maxImpact"`
	PacksChanged int    `json:"packsChanged"`
	LastModified string `json:"lastModified"`
	Diff         *Diff  `json:"diff"`
	Error        *Error `json:"error"`
}

// Diff is strata-diff/1, with no a, no b, no variables, and no clusterVariables.
type Diff struct {
	Schema          string      `json:"schema"`
	Same            bool        `json:"same"`
	SameContent     bool        `json:"sameContent"`
	EditedInPlace   bool        `json:"editedInPlace"`
	RootA           string      `json:"rootA"`
	RootB           string      `json:"rootB"`
	Summary         DiffSummary `json:"summary"`
	Packs           []Pack      `json:"packs"`
	UpstreamUnknown []string    `json:"upstreamUnknown"`
	ImageIDsChanged []KeyOnly   `json:"imageIdsChanged"`
	Verify          *Verify     `json:"verify"`
}

// KeyOnly names a pack, and keeps nothing else of the entry.
type KeyOnly struct {
	Key string `json:"key"`
}

// DiffSummary holds the counts of a comparison.
type DiffSummary struct {
	PacksChanged  int    `json:"packsChanged"`
	PacksAdded    int    `json:"packsAdded"`
	PacksRemoved  int    `json:"packsRemoved"`
	ImagesChanged int    `json:"imagesChanged"`
	ChartsChanged int    `json:"chartsChanged"`
	MaxImpact     string `json:"maxImpact"`
}

// Pack is one entry of packs[] in strata-diff/1.
type Pack struct {
	Key             string   `json:"key"`
	Name            string   `json:"name"`
	Layer           string   `json:"layer"`
	Status          string   `json:"status"`
	Identity        bool     `json:"identity"`
	Upstream        bool     `json:"upstream"`
	Authored        bool     `json:"authored"`
	Images          bool     `json:"images"`
	Charts          bool     `json:"charts"`
	UpstreamUnknown bool     `json:"upstreamUnknown"`
	Impact          Impact   `json:"impact"`
	A               *Side    `json:"a"`
	B               *Side    `json:"b"`
	Values          Values   `json:"values"`
	Manifests       []Named  `json:"manifests"`
	ParamChanges    []Named  `json:"paramChanges"`
	ImageChanges    []Change `json:"imageChanges"`
	ChartChanges    []Change `json:"chartChanges"`
	Masked          []string `json:"masked"`
}

// Impact is the level of a pack change, in the words of strata.
type Impact struct {
	Level string `json:"level"`
	Text  string `json:"text"`
}

// Side is one side of a pack.
type Side struct {
	Tag      string `json:"tag"`
	Version  string `json:"version"`
	Upstream string `json:"upstream"`
}

// Values holds the changes of the pack values. Only the detail tier sends Diff.
type Values struct {
	Parseable    bool   `json:"parseable"`
	ChangedCount int    `json:"changedCount"`
	AddedCount   int    `json:"addedCount"`
	RemovedCount int    `json:"removedCount"`
	Diff         []Leaf `json:"diff"`
}

// Leaf is one changed value. Old and New are a scalar or an empty container.
type Leaf struct {
	Op   string `json:"op"`
	Path string `json:"path"`
	Old  any    `json:"old,omitempty"`
	New  any    `json:"new,omitempty"`
}

// Named keeps the operation and the name of a change, and not its content.
type Named struct {
	Op   string `json:"op"`
	Name string `json:"name"`
}

// Change is a change of an image or a chart: a repository or a chart name, and a tag or a version.
type Change struct {
	Op   string `json:"op"`
	Name string `json:"name"`
	Old  string `json:"old,omitempty"`
	New  string `json:"new,omitempty"`
}

// Verify is the verify block of strata-diff/1.
type Verify struct {
	Status          string   `json:"status"`
	RecordedAt      string   `json:"recordedAt"`
	CheckedAt       string   `json:"checkedAt"`
	UpstreamUnknown []string `json:"upstreamUnknown"`
	OverwrittenBy   string   `json:"overwrittenBy"`
	RenamedTo       string   `json:"renamedTo"`
	Gone            bool     `json:"gone"`
	LastModified    string   `json:"lastModified"`
	Who             []Who    `json:"who"`
}

// Who is one write that the audit log of Palette holds.
type Who struct {
	Time      string `json:"time"`
	Actor     string `json:"actor"`
	ActorType string `json:"actorType"`
	Action    string `json:"action"`
}

// Behind is strata-behind/1.
type Behind struct {
	Schema string `json:"schema"`
	Tool   Tool   `json:"tool"`
	Pins   []Pin  `json:"pins"`
}

// Pin is one pack, registry, pinned version, and tag.
type Pin struct {
	Name         string        `json:"name"`
	Layer        string        `json:"layer"`
	Registry     string        `json:"registry"`
	RegistryUID  string        `json:"registryUid"`
	Version      string        `json:"version"`
	Tag          string        `json:"tag"`
	Floating     bool          `json:"floating"`
	Resolved     string        `json:"resolved"`
	PinnedState  string        `json:"pinnedState"`
	PinnedBy     []PinnedBy    `json:"pinnedBy"`
	Newer        []Newer       `json:"newer"`
	Behind       bool          `json:"behind"`
	Error        *Error        `json:"error"`
	RegistrySync *RegistrySync `json:"registrySync"`
}

// PinnedBy names a record that pins a pack. Subject is name@version of the profile.
type PinnedBy struct {
	Subject string `json:"subject"`
}

// Newer is one version that a person can move to.
type Newer struct {
	Version string   `json:"version"`
	Listed  string   `json:"listed"`
	Drops   []string `json:"drops"`
}

// RegistrySync says how old the copy of a registry in Palette is.
type RegistrySync struct {
	LastSyncedTime string `json:"lastSyncedTime"`
	Status         string `json:"status"`
}

// Error is the error object of a document, and of the stderr line of a run that exits 2. It has no
// message, because the message of the kind api can hold the body of an answer from Palette.
type Error struct {
	Kind   string `json:"kind"`
	Status int    `json:"status"`
}

// Record is the part of a strata-snapshot/1 profile record that the observer reads.
type Record struct {
	Schema string `json:"schema"`
	Tool   Tool   `json:"tool"`
	Source struct {
		UID        string `json:"uid"`
		ProjectUID string `json:"projectUid"`
		Name       string `json:"name"`
		Version    string `json:"version"`
		Type       string `json:"type"`
		FetchedAt  string `json:"fetchedAt"`
		// Cluster is here only so that the observer can refuse a cluster record. It keeps no other field.
		Cluster *struct {
			UID string `json:"uid"`
		} `json:"cluster"`
	} `json:"source"`
	Root  string       `json:"root"`
	Packs []RecordPack `json:"packs"`
}

// RecordPack is one pack of a record. Images is a list of references. Values are not here.
type RecordPack struct {
	Key                  string   `json:"key"`
	Layer                string   `json:"layer"`
	Name                 string   `json:"name"`
	Type                 string   `json:"type"`
	Version              string   `json:"version"`
	RegistryUID          string   `json:"registryUid"`
	RegistryProviderType string   `json:"registryProviderType"`
	Images               []string `json:"images"`
	Hash                 PackHash `json:"hash"`
}

// PackHash holds the upstream digest that Palette lists for a pack.
type PackHash struct {
	UpstreamRaw    string `json:"upstreamRaw"`
	UpstreamSource string `json:"upstreamSource"`
}

// ClusterRead is the part of a cluster snapshot that the observer reads. It has no variables.
type ClusterRead struct {
	Source struct {
		Cluster struct {
			UID            string           `json:"uid"`
			Name           string           `json:"name"`
			Profiles       []ClusterProfile `json:"profiles"`
			UpdatesPending bool             `json:"updatesPending"`
			RepavePending  bool             `json:"repavePending"`
		} `json:"cluster"`
	} `json:"source"`
}

// ClusterProfile is one profile that a cluster runs.
type ClusterProfile struct {
	Name    string `json:"name"`
	Version string `json:"version"`
	Type    string `json:"type"`
}
