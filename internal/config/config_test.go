package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadMissing(t *testing.T) {
	cfg, err := Load(filepath.Join(t.TempDir(), "config.yaml"))
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	if cfg == nil {
		t.Fatal("Load() returned nil for missing file")
	}
}

func TestSaveAndLoad(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.yaml")
	orig := &Config{}

	if err := Save(orig, path); err != nil {
		t.Fatalf("Save() error = %v", err)
	}

	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load() error = %v", err)
	}
	_ = got // compare fields once you've added them to Config
}

func TestLoadInvalidYAML(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.yaml")
	if err := os.WriteFile(path, []byte(":::invalid yaml"), 0o644); err != nil {
		t.Fatal(err)
	}

	_, err := Load(path)
	if err == nil {
		t.Fatal("Load() expected error for invalid YAML")
	}
}

func TestBoolVal(t *testing.T) {
	tests := map[string]struct {
		ptr      *bool
		fallback bool
		want     bool
	}{
		"nil uses fallback true":  {ptr: nil, fallback: true, want: true},
		"nil uses fallback false": {ptr: nil, fallback: false, want: false},
		"ptr true":                {ptr: BoolPtr(true), fallback: false, want: true},
		"ptr false":               {ptr: BoolPtr(false), fallback: true, want: false},
	}

	for name, tt := range tests {
		t.Run(name, func(t *testing.T) {
			if got := BoolVal(tt.ptr, tt.fallback); got != tt.want {
				t.Errorf("BoolVal() = %v, want %v", got, tt.want)
			}
		})
	}
}
