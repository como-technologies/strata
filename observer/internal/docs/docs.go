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
	RootRecorded string `json:"rootRecorded"`
	RootLive     string `json:"rootLive"`
	MaxImpact    string `json:"maxImpact"`
	Diff         *Diff  `json:"diff"`
	Error        *Error `json:"error"`
}

// Diff is strata-diff/1, with no a, no b, no variables, and no clusterVariables.
type Diff struct {
	Schema          string      `json:"schema"`
	Same            bool        `json:"same"`
	SameContent     bool        `json:"sameContent"`
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
	ImagesChanged int    `json:"imagesChanged"`
	ChartsChanged int    `json:"chartsChanged"`
	MaxImpact     string `json:"maxImpact"`
}

// Pack is one entry of packs[] in strata-diff/1. TextTooLarge is not a field of strata: the observer
// sets it when it drops the unified text of the pack because the text is above the cap.
type Pack struct {
	Key             string     `json:"key"`
	Name            string     `json:"name"`
	Layer           string     `json:"layer"`
	Status          string     `json:"status"`
	Identity        bool       `json:"identity"`
	Upstream        bool       `json:"upstream"`
	Authored        bool       `json:"authored"`
	Images          bool       `json:"images"`
	Charts          bool       `json:"charts"`
	UpstreamUnknown bool       `json:"upstreamUnknown"`
	Impact          Impact     `json:"impact"`
	Values          Values     `json:"values"`
	Manifests       []Manifest `json:"manifests"`
	ParamChanges    []Named    `json:"paramChanges"`
	ImageChanges    []Change   `json:"imageChanges"`
	ChartChanges    []Change   `json:"chartChanges"`
	Masked          []string   `json:"masked"`
	TextTooLarge    bool       `json:"-"`
}

// Impact is the level of a pack change, in the words of strata.
type Impact struct {
	Level string `json:"level"`
	Text  string `json:"text"`
}

// Values holds the changes of the pack values. Only the detail tier sends Diff and Unified. Unified is
// the text of the values as `--full` prints it, in the unified form.
type Values struct {
	Parseable bool   `json:"parseable"`
	Diff      []Leaf `json:"diff"`
	Unified   string `json:"unified"`
}

// Leaf is one changed value. Old and New are a scalar or an empty container.
type Leaf struct {
	Op   string `json:"op"`
	Path string `json:"path"`
	Old  any    `json:"old,omitempty"`
	New  any    `json:"new,omitempty"`
}

// Paths lists the path of each changed value, in the order of the diff.
func (v Values) Paths() []string {
	paths := make([]string, 0, len(v.Diff))
	for _, l := range v.Diff {
		paths = append(paths, l.Path)
	}
	return paths
}

// Named keeps the operation and the name of a change, and not its content.
type Named struct {
	Op   string `json:"op"`
	Name string `json:"name"`
}

// Manifest is a change of a manifest: the operation, the name, and the text of the change in the unified
// form. Only the detail tier sends Unified.
type Manifest struct {
	Op      string `json:"op"`
	Name    string `json:"name"`
	Unified string `json:"unified"`
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
	UpstreamUnknown []string `json:"upstreamUnknown"`
	OverwrittenBy   string   `json:"overwrittenBy"`
	RenamedTo       string   `json:"renamedTo"`
	Gone            bool     `json:"gone"`
	Who             []Who    `json:"who"`
	WhoError        string   `json:"whoError"`
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
	Floating     bool          `json:"floating"`
	PinnedState  string        `json:"pinnedState"`
	PinnedBy     []PinnedBy    `json:"pinnedBy"`
	Newer        []Newer       `json:"newer"`
	Behind       bool          `json:"behind"`
	Bump         *Bump         `json:"bump"`
	Error        *Error        `json:"error"`
	RegistrySync *RegistrySync `json:"registrySync"`
}

// Bump is what a move to the newest version does, as `behind` reads it with a question to Palette.
// PaletteValidationError holds the words of a request that failed. They can hold the body of an answer
// of Palette, so only the status leaves the observer: see StatusOf.
type Bump struct {
	To                     string      `json:"to"`
	PaletteValidation      *Validation `json:"paletteValidation"`
	PaletteValidationError string      `json:"paletteValidationError"`
}

// Validation is the answer of Palette to the question about a move: it accepts when it gives no error.
type Validation struct {
	Accepts bool      `json:"accepts"`
	Errors  []Refusal `json:"errors"`
}

// Refusal is one error that Palette gives for a pack. Only the detail tier sends it.
type Refusal struct {
	Pack     string   `json:"pack"`
	Code     string   `json:"code"`
	Message  string   `json:"message"`
	Clusters []string `json:"clusters"`
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
		ProjectUID string `json:"projectUid"`
		Name       string `json:"name"`
		Version    string `json:"version"`
		Type       string `json:"type"`
		// Cluster is here only so that the observer can refuse a cluster record. It keeps no field.
		Cluster *struct{} `json:"cluster"`
	} `json:"source"`
	// Root says whether a record changed: the observer keeps the last good answer of a part only for the
	// same root.
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
	Schema string `json:"schema"`
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

// CapText drops the unified text of each pack whose text is above limit bytes, and marks the pack with
// TextTooLarge. A pack keeps all of its text or none: half of a diff would mislead.
func (d *Diff) CapText(limit int) {
	for i := range d.Packs {
		p := &d.Packs[i]
		size := len(p.Values.Unified)
		for _, m := range p.Manifests {
			size += len(m.Unified)
		}
		if size <= limit {
			continue
		}
		p.Values.Unified = ""
		manifests := make([]Manifest, len(p.Manifests))
		for j, m := range p.Manifests {
			manifests[j] = Manifest{Op: m.Op, Name: m.Name}
		}
		p.Manifests = manifests
		p.TextTooLarge = true
	}
}
