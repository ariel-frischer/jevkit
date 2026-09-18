package version

import "testing"

func TestIsDevBuild(t *testing.T) {
	tests := map[string]struct {
		version string
		want    bool
	}{
		"dev build":     {version: "dev", want: true},
		"release build": {version: "1.0.0", want: false},
		"pre-release":   {version: "1.0.0-rc1", want: false},
	}

	for name, tt := range tests {
		t.Run(name, func(t *testing.T) {
			orig := Version
			defer func() { Version = orig }()

			Version = tt.version
			if got := IsDevBuild(); got != tt.want {
				t.Errorf("IsDevBuild() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestDefaultValues(t *testing.T) {
	if Version != "dev" {
		t.Errorf("default Version = %q, want %q", Version, "dev")
	}
	if Commit != "unknown" {
		t.Errorf("default Commit = %q, want %q", Commit, "unknown")
	}
	if BuildDate != "unknown" {
		t.Errorf("default BuildDate = %q, want %q", BuildDate, "unknown")
	}
}
