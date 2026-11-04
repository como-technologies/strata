package model

import (
	"regexp"

	"github.com/como-technologies/strata/observer/internal/docs"
)

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
	groups
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

// FoldOf puts the changed value at each path in one group, and counts the groups.
func FoldOf(paths []string) Fold {
	var n [groups]int
	for _, p := range paths {
		n[GroupOf(p)]++
	}
	return Fold{Settings: n[GroupSettings], Follows: n[GroupFollows], Repeats: n[GroupRepeats]}
}

// Split puts each changed value in the list of its group, in the order of the diff. The three lists
// hold the values that FoldOf counts. Only the detail tier of the API sends them.
func Split(v docs.Values) (settings, follows, repeats []docs.Leaf) {
	var lists [groups][]docs.Leaf
	for g := range lists {
		lists[g] = []docs.Leaf{}
	}
	for _, l := range v.Diff {
		g := GroupOf(l.Path)
		lists[g] = append(lists[g], l)
	}
	return lists[GroupSettings], lists[GroupFollows], lists[GroupRepeats]
}
