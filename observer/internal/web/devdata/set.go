package devdata

import (
	"errors"
	"fmt"
	"io/fs"
	"path"
	"slices"
	"time"

	"github.com/como-technologies/strata/observer/internal/alert"
	"github.com/como-technologies/strata/observer/internal/api"
	"github.com/como-technologies/strata/observer/internal/model"
)

// StateFleet is the set of a set of clusters, with --clusters. It holds the answer of /v1/clusters in clusters.json,
// the answer of /v1/shared in shared.json, and the rules of the alerts of the set. The directory members holds one set
// of each cluster, by its uid, and each one names a set of one cluster in its base. The cluster quarry-1 runs the
// observer and holds the finding of ok. ledger is OK with one cause of Not checked, and the key of the project of search
// is refused. A second selector selects no cluster.
const StateFleet State = "fleet"

// SetStates lists the sets of a set of clusters. They are not in States: the routes of one cluster of a set are under
// the uid of the cluster.
var SetStates = []State{StateFleet}

// The files of a set of clusters, and the directory of its clusters.
const (
	fileClusters = "clusters.json"
	fileShared   = "shared.json"
	dirMembers   = "members"
)

// SetData holds the answer of each route of a set of clusters. It is an api.Set.
type SetData struct {
	Summary    model.SetSummary
	SharedList api.SharedView
	AlertRules alert.View
	// Members holds the set of each cluster, by its uid.
	Members map[string]*Set
}

// LoadSet reads the set of a set of clusters, and the set of each of its clusters. With a now that is not zero, it moves
// each time by now less Reference. Each cluster gets the rules of the alerts of the set.
func LoadSet(fsys fs.FS, state State, now time.Time) (*SetData, error) {
	s := SetData{AlertRules: alert.Unset(), Members: map[string]*Set{}}
	for _, f := range []struct {
		name string
		into any
	}{{fileClusters, &s.Summary}, {fileShared, &s.SharedList}, {fileAlerts, &s.AlertRules}} {
		raw, err := fs.ReadFile(fsys, path.Join(string(state), f.name))
		if f.name == fileAlerts && errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, fmt.Errorf("state %s: %w", state, err)
		}
		if !now.IsZero() {
			raw = shiftTimes(raw, now.Sub(Reference))
		}
		if err := Decode(raw, f.into); err != nil {
			return nil, fmt.Errorf("state %s, %s: %w", state, f.name, err)
		}
	}
	entries, err := fs.ReadDir(fsys, path.Join(string(state), dirMembers))
	if err != nil {
		return nil, fmt.Errorf("state %s: %w", state, err)
	}
	for _, e := range entries {
		m, err := Load(fsys, State(path.Join(string(state), dirMembers, e.Name())), now)
		if err != nil {
			return nil, err
		}
		m.AlertRules = s.AlertRules
		s.Members[e.Name()] = m
	}
	return &s, nil
}

// Clusters gives the summary of the set.
func (s *SetData) Clusters() model.SetSummary {
	return s.Summary
}

// Shared gives the profile versions that the clusters of the set run.
func (s *SetData) Shared() []model.Shared {
	return slices.Clone(s.SharedList.Shared)
}

// Member gives the set of one cluster.
func (s *SetData) Member(uid string) (api.Source, bool) {
	m, ok := s.Members[uid]
	if !ok {
		return nil, false
	}
	return m, true
}

// Alerts gives the rules of the alerts of the set.
func (s *SetData) Alerts() alert.View {
	return s.AlertRules
}

// StartCycle answers the click "Check now" as the set of one cluster does.
func (s *SetData) StartCycle(name string) (model.Start, bool) {
	return (&Set{}).StartCycle(name)
}
