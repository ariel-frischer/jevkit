// Package version holds jevkit version information.
// Separate package to avoid import cycles.
package version

var (
	// Set via ldflags during build
	Version   = "dev"
	Commit    = "unknown"
	BuildDate = "unknown"
)

func IsDevBuild() bool {
	return Version == "dev"
}
