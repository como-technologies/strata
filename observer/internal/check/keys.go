package check

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"github.com/como-technologies/strata/observer/internal/runner"
)

// HomeKeyFile is the entry of the Secret strata-observer-key that holds the key of the project of the ConfigMap. Each
// other entry of a set of clusters holds the key of one more project, with the uid of the project as its name.
const HomeKeyFile = "api-key"

// readKeys reads the keys of a set from the directory of the mounted Secret. The entry HomeKeyFile is the key of the
// project home, and it must be there. A project uid can start with a digit, so it is not a portable name of an
// environment variable. So the observer reads the files, again at the start of each cycle: the kubelet updates a
// mounted Secret. The kubelet also writes entries whose names start with a dot, and they are no key. The keys come in
// the order of their project.
func readKeys(dir, home string) ([]runner.Key, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var keys []runner.Key
	var homeFound bool
	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") {
			continue
		}
		info, err := os.Stat(filepath.Join(dir, name))
		if err != nil || info.IsDir() {
			continue
		}
		raw, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			return nil, err
		}
		secret := strings.TrimSpace(string(raw))
		project := name
		if name == HomeKeyFile {
			project, homeFound = home, true
		}
		if secret == "" {
			return nil, fmt.Errorf("the key of the project %s is empty", project)
		}
		if i := slices.IndexFunc(keys, func(k runner.Key) bool { return k.Project == project }); i >= 0 {
			return nil, fmt.Errorf("two keys are for the project %s", project)
		}
		keys = append(keys, runner.Key{Project: project, Secret: secret})
	}
	if !homeFound {
		return nil, errors.New("the keys have no entry " + HomeKeyFile)
	}
	slices.SortFunc(keys, func(a, b runner.Key) int { return strings.Compare(a.Project, b.Project) })
	return keys, nil
}

// keyOfProject finds the key of a project in keys.
func keyOfProject(keys []runner.Key, project string) (runner.Key, bool) {
	i := slices.IndexFunc(keys, func(k runner.Key) bool { return k.Project == project })
	if i < 0 {
		return runner.Key{}, false
	}
	return keys[i], true
}
