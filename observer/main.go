// Command strata-observer runs the strata binary on two timers and serves the answers on a page.
package main

import (
	"fmt"
	"io"
	"os"
)

// version is set by the build with -ldflags "-X main.version=...".
var version = "dev"

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

// run gives the exit code. The codes have the meaning that they have in strata: 2 is an error.
func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 1 && args[0] == "version" {
		fmt.Fprintln(stdout, "strata-observer", version)
		return 0
	}
	fmt.Fprintln(stderr, "usage: strata-observer version")
	return 2
}
