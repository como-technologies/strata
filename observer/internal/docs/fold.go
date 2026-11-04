package docs

import "regexp"

var (
	repeatsTheLists = regexp.MustCompile(`^pack\.content\.(images|charts)(\[|\.|$)`)
	followsVersion  = regexp.MustCompile(`(?i)(^|\.)(repository|registry|tag|image|imageTag|imageRepository|imageRegistry|useDigest|digest)(\[[^\]]*\])?$`)
)

// Fold counts the changed values of a pack in three groups. It changes no flag and no level.
type Fold struct {
	Settings int `json:"settings"`
	Follows  int `json:"followsTheVersion"`
	Repeats  int `json:"repeatsTheLists"`
}

// Group is the group of a changed value in a fold.
type Group int

// The groups of a changed value.
const (
	GroupSettings Group = iota
	GroupFollows
	GroupRepeats
)

// GroupOf gives the group of the changed value at a path.
func GroupOf(path string) Group {
	switch {
	case repeatsTheLists.MatchString(path):
		return GroupRepeats
	case followsVersion.MatchString(path):
		return GroupFollows
	}
	return GroupSettings
}

// FoldOf puts each changed value in one group.
func FoldOf(v Values) Fold {
	var f Fold
	for _, l := range v.Diff {
		switch GroupOf(l.Path) {
		case GroupRepeats:
			f.Repeats++
		case GroupFollows:
			f.Follows++
		default:
			f.Settings++
		}
	}
	return f
}

// Split puts each changed value in the list of its group, in the order of the diff. The three lists
// hold the same values that FoldOf counts. Only the detail tier of the API sends them.
func Split(v Values) (settings, follows, repeats []Leaf) {
	settings, follows, repeats = []Leaf{}, []Leaf{}, []Leaf{}
	for _, l := range v.Diff {
		switch GroupOf(l.Path) {
		case GroupRepeats:
			repeats = append(repeats, l)
		case GroupFollows:
			follows = append(follows, l)
		default:
			settings = append(settings, l)
		}
	}
	return settings, follows, repeats
}
