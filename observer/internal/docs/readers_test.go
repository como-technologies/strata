package docs

import (
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

// forwarded lists the fields that no code reads, and that the detail tier sends as they are. The page
// reads them there. The summary tier sends none of them.
var forwarded = []string{
	"Impact.Level", "Impact.Text",
	"Leaf.Op", "Leaf.Old", "Leaf.New",
	"Named.Op", "Named.Name",
	"Change.Op", "Change.Name", "Change.Old", "Change.New",
}

// fieldAt names a field by the file, the line, and the column of its declaration. The package docs is
// type-checked twice, once for its own code and once as an import of the other packages. The two copies
// share no object, but they share the position.
func fieldAt(fset *token.FileSet, v *types.Var) string {
	p := fset.Position(v.Pos())
	return fmt.Sprintf("%s:%d:%d", filepath.Base(p.Filename), p.Line, p.Column)
}

// checkDir type-checks the code of one directory that is no test, and gives what it learned.
func checkDir(t *testing.T, fset *token.FileSet, imp types.Importer, dir string) []*types.Info {
	t.Helper()
	pkgs, err := parser.ParseDir(fset, dir, func(fi os.FileInfo) bool { return !strings.HasSuffix(fi.Name(), "_test.go") }, 0)
	if err != nil {
		t.Fatal(err)
	}
	var out []*types.Info
	for _, pkg := range pkgs {
		var files []*ast.File
		for _, f := range pkg.Files {
			files = append(files, f)
		}
		info := &types.Info{Uses: map[*ast.Ident]types.Object{}, Defs: map[*ast.Ident]types.Object{}}
		conf := types.Config{Importer: imp, Error: func(err error) { t.Errorf("%s: %v", dir, err) }}
		if _, err := conf.Check(pkg.Name, fset, files, info); err != nil {
			t.Fatalf("%s: %v", dir, err)
		}
		out = append(out, info)
	}
	return out
}

// structFields names each field of a struct type of the package, as Type.Field. A field of an
// anonymous struct has the names of its parents: Record.Source.Name. A field of a named type belongs
// to that type only.
func structFields(prefix string, s *types.Struct, fset *token.FileSet, out map[string]string) {
	for i := range s.NumFields() {
		f := s.Field(i)
		name := prefix + "." + f.Name()
		out[name] = fieldAt(fset, f)
		typ := f.Type()
		if ptr, ok := typ.(*types.Pointer); ok {
			typ = ptr.Elem()
		}
		if inner, ok := typ.(*types.Struct); ok {
			structFields(name, inner, fset, out)
		}
	}
}

// The package docs is the allow-list of the fields that the observer reads. A field that no code
// reads is a field that enters the process for no reason. The test type-checks the code of the
// module, so a name that two types share does not hide an unread field.
func TestEachFieldOfTheAllowListHasAReader(t *testing.T) {
	fset := token.NewFileSet()
	imp := importer.ForCompiler(fset, "source", nil)
	var dirs []string
	err := filepath.WalkDir("../..", func(path string, d os.DirEntry, err error) error {
		switch {
		case err != nil:
			return err
		case d.IsDir() && d.Name() == "testdata":
			return filepath.SkipDir
		case d.IsDir():
			dirs = append(dirs, path)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	read := map[string]bool{}
	declared := map[string]string{}
	for _, dir := range dirs {
		for _, info := range checkDir(t, fset, imp, dir) {
			for _, obj := range info.Uses {
				if v, ok := obj.(*types.Var); ok && v.IsField() {
					read[fieldAt(fset, v)] = true
				}
			}
			if filepath.Base(dir) != "docs" {
				continue
			}
			for _, obj := range info.Defs {
				if tn, ok := obj.(*types.TypeName); ok && tn.Parent() == tn.Pkg().Scope() {
					if s, ok := tn.Type().Underlying().(*types.Struct); ok {
						structFields(tn.Name(), s, fset, declared)
					}
				}
			}
		}
	}
	if len(declared) < 50 {
		t.Fatalf("the test found only %d fields of the package docs", len(declared))
	}
	for _, name := range slices.Sorted(maps.Keys(declared)) {
		if at := declared[name]; !read[at] && !slices.Contains(forwarded, name) {
			t.Errorf("no code reads the field %s (%s): remove it from the package docs", name, at)
		}
	}
	for _, name := range forwarded {
		if _, ok := declared[name]; !ok {
			t.Errorf("the list forwarded names %s, and the package docs has no such field", name)
		}
	}
}
