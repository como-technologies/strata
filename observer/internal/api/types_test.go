package api

import (
	"reflect"
	"testing"

	"github.com/como-technologies/strata/observer/internal/docs"
	"github.com/como-technologies/strata/observer/internal/model"
)

// valueTypes are the types that hold a changed value. Only PackDiff of the detail tier can use them.
var valueTypes = []reflect.Type{
	reflect.TypeOf(docs.Leaf{}), reflect.TypeOf(docs.Values{}), reflect.TypeOf(docs.Diff{}),
	reflect.TypeOf(docs.Pack{}), reflect.TypeOf(docs.Change{}),
}

// holds says which of the types a type uses in a field, at any depth. A path is the chain of fields.
func holds(t reflect.Type, banned []reflect.Type, path string, seen map[reflect.Type]bool, found *[]string) {
	for _, b := range banned {
		if t == b {
			*found = append(*found, path+" is "+t.String())
			return
		}
	}
	if seen[t] {
		return
	}
	seen[t] = true
	switch t.Kind() {
	case reflect.Pointer, reflect.Slice, reflect.Array:
		holds(t.Elem(), banned, path, seen, found)
	case reflect.Map:
		holds(t.Key(), banned, path+"[key]", seen, found)
		holds(t.Elem(), banned, path+"[]", seen, found)
	case reflect.Struct:
		for i := 0; i < t.NumField(); i++ {
			f := t.Field(i)
			holds(f.Type, banned, path+"."+f.Name, seen, found)
		}
	case reflect.Interface:
		// Only docs.Leaf uses an interface (Old and New), and the walk stops at Leaf itself.
		*found = append(*found, path+" is an interface, so it can hold any value")
	}
}

// The summary tier has types of its own, and none of them has a field of type docs.Leaf. The compiler
// and this test keep a changed value out of a summary route.
func TestNoTypeOfTheSummaryTierHoldsAChangedValue(t *testing.T) {
	summary := map[string]reflect.Type{
		"model.Summary":  reflect.TypeOf(model.Summary{}),
		"ClusterView":    reflect.TypeOf(ClusterView{}),
		"FindingsView":   reflect.TypeOf(FindingsView{}),
		"ProfilesView":   reflect.TypeOf(ProfilesView{}),
		"ProfileView":    reflect.TypeOf(ProfileView{}),
		"NewerView":      reflect.TypeOf(NewerView{}),
		"NotCheckedView": reflect.TypeOf(NotCheckedView{}),
		"EventsView":     reflect.TypeOf(EventsView{}),
	}
	for name, typ := range summary {
		var found []string
		holds(typ, valueTypes, name, map[reflect.Type]bool{}, &found)
		for _, f := range found {
			t.Errorf("%s", f)
		}
	}
}

// The test above must be able to fail: PackDiff is the detail type, and it holds docs.Leaf.
func TestTheWalkSeesTheLeafOfTheDetailType(t *testing.T) {
	var found []string
	holds(reflect.TypeOf(PackDiff{}), valueTypes, "PackDiff", map[reflect.Type]bool{}, &found)
	if len(found) == 0 {
		t.Error("the walk found no value type in PackDiff")
	}
}

// Every type that a route serves is a view or a part of the summary: a route cannot serve a docs type.
func TestEachSummaryViewIsAPartOfTheSummary(t *testing.T) {
	sum := reflect.TypeOf(model.Summary{})
	fields := map[string]reflect.Type{}
	for i := 0; i < sum.NumField(); i++ {
		f := sum.Field(i)
		fields[f.Tag.Get("json")] = f.Type
	}
	for _, view := range []reflect.Type{reflect.TypeOf(ClusterView{}), reflect.TypeOf(FindingsView{}), reflect.TypeOf(NewerView{}),
		reflect.TypeOf(NotCheckedView{}), reflect.TypeOf(EventsView{})} {
		for i := 0; i < view.NumField(); i++ {
			f := view.Field(i)
			name := f.Tag.Get("json")
			if want, ok := fields[name]; !ok || want != f.Type {
				t.Errorf("%s.%s is %v, and the summary has %v", view.Name(), f.Name, f.Type, want)
			}
		}
	}
}
