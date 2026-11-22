package check

import (
	"fmt"
	"strings"

	"github.com/como-technologies/strata/observer/internal/runner"
)

// The prefixes of a selector of the flag --clusters. They are the set refs that strata reads: the clusters with
// some labels, and the clusters that a saved tag filter of Palette selects.
const (
	tagPrefix    = "tag:"
	filterPrefix = "filter:"
	// labelJoin joins two labels that must all hold, as in tag:A=B+C=D.
	labelJoin = "+"
)

// Selector is one selector of a set of clusters: tag:KEY=VALUE, tag:KEY, tag:A=B+C=D, or filter:NAME. Palette owns
// the language of a saved filter, so a condition that a selector cannot hold goes into a saved filter.
type Selector struct {
	// Text is the selector as the flag gives it.
	Text string
	// Labels holds each label of a tag selector, KEY=VALUE or KEY. Filter is the name or the uid of a saved filter.
	Labels []string
	Filter string
}

// ParseSelector reads one selector. A selector with another prefix, an empty part, or a part that starts with a
// dash is an error: strata reads a part that starts with a dash as a flag.
func ParseSelector(text string) (Selector, error) {
	s := Selector{Text: text}
	var parts []string
	switch {
	case strings.HasPrefix(text, tagPrefix):
		s.Labels = strings.Split(strings.TrimPrefix(text, tagPrefix), labelJoin)
		parts = s.Labels
	case strings.HasPrefix(text, filterPrefix):
		s.Filter = strings.TrimPrefix(text, filterPrefix)
		parts = []string{s.Filter}
	default:
		return s, fmt.Errorf("the selector %q is not %sKEY=VALUE, %sKEY, or %sNAME", text, tagPrefix, tagPrefix, filterPrefix)
	}
	for _, part := range parts {
		switch {
		case part == "":
			return s, fmt.Errorf("the selector %q has an empty part", text)
		case strings.HasPrefix(part, "-"):
			return s, fmt.Errorf("the selector %q has a part that starts with a dash", text)
		}
	}
	return s, nil
}

// ParseSelectors reads the values of the flag --clusters. One value can hold many selectors, one on each line, as
// the key clusters of the ConfigMap does. A blank line is no selector.
func ParseSelectors(values []string) ([]Selector, error) {
	var out []Selector
	for _, v := range values {
		for _, line := range strings.Split(v, "\n") {
			line = strings.TrimSpace(line)
			if line == "" {
				continue
			}
			s, err := ParseSelector(line)
			if err != nil {
				return nil, err
			}
			out = append(out, s)
		}
	}
	return out, nil
}

// args gives the command that finds the clusters of the selector in the project of a key. The search of one project
// answers a key with a role in that project. A search at tenant scope needs a tenant role.
func (s Selector) args(project string) []string {
	args := []string{runner.CmdList, "clusters"}
	for _, l := range s.Labels {
		args = append(args, runner.FlagTag, l)
	}
	if s.Filter != "" {
		args = append(args, runner.FlagFilter, s.Filter)
	}
	return append(args, runner.FlagProject, project, "--json", "--compact", "-q")
}
