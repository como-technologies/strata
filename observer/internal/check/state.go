package check

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"time"

	"github.com/como-technologies/strata/observer/internal/model"
)

// stateFile names the file of the state in the directory of the state.
const stateFile = "state.json"

// State is what the observer keeps between two runs. It holds the first sight of each finding, the
// counts of each move, and the failed runs of each timer. It also holds the first digest of each image.
// It holds no value of a variable, no old value, and no new value, so a copy of the file shows
// nothing of the cluster.
type State struct {
	StartedAt time.Time               `json:"startedAt"`
	Findings  map[string]FindingState `json:"findings"`
	Moves     map[string]model.Move   `json:"moves"`
	Content   ClockState              `json:"content"`
	Versions  ClockState              `json:"versions"`
	Images    ImagesState             `json:"images"`
}

// ImagesState is what the observer keeps of the pods. First maps the id of an image, ImageKey, to the
// first digest that the observer saw for it. Revision is the revision of the records that the digests
// belong to. It holds no reference, no pod, and no namespace.
type ImagesState struct {
	Revision string            `json:"revision"`
	First    map[string]string `json:"first"`
}

// FindingState is what the observer keeps of a finding.
type FindingState struct {
	FirstSeen time.Time `json:"firstSeen"`
}

// ClockState is what the observer keeps of a timer.
type ClockState struct {
	FailedRuns int             `json:"failedRuns"`
	LastError  *model.ErrorRef `json:"lastError"`
}

func emptyState() State {
	return State{Findings: map[string]FindingState{}, Moves: map[string]model.Move{}, Images: ImagesState{First: map[string]string{}}}
}

// loadState reads the state. A file that is absent gives an empty state. A file that cannot be read
// gives an empty state and the error, so that the caller writes a log line.
func loadState(dir string) (State, error) {
	raw, err := os.ReadFile(filepath.Join(dir, stateFile))
	if errors.Is(err, os.ErrNotExist) {
		return emptyState(), nil
	}
	if err != nil {
		return emptyState(), err
	}
	s := emptyState()
	if err := json.Unmarshal(raw, &s); err != nil {
		return emptyState(), err
	}
	if s.Findings == nil {
		s.Findings = map[string]FindingState{}
	}
	if s.Moves == nil {
		s.Moves = map[string]model.Move{}
	}
	if s.Images.First == nil {
		s.Images.First = map[string]string{}
	}
	return s, nil
}

// saveState writes the state to a temporary file and renames it, so that a reader never sees half
// of the file.
func saveState(dir string, s State) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	var raw bytes.Buffer
	enc := json.NewEncoder(&raw)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(s); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(dir, stateFile+".*.tmp")
	if err != nil {
		return err
	}
	_, werr := tmp.Write(raw.Bytes())
	cerr := tmp.Close()
	if err := errors.Join(werr, cerr); err != nil {
		return errors.Join(err, os.Remove(tmp.Name()))
	}
	if err := os.Rename(tmp.Name(), filepath.Join(dir, stateFile)); err != nil {
		return errors.Join(err, os.Remove(tmp.Name()))
	}
	return nil
}
