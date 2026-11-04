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

// FoldOf puts each changed value in one group.
func FoldOf(v Values) Fold {
	var f Fold
	for _, l := range v.Diff {
		switch {
		case repeatsTheLists.MatchString(l.Path):
			f.Repeats++
		case followsVersion.MatchString(l.Path):
			f.Follows++
		default:
			f.Settings++
		}
	}
	return f
}
