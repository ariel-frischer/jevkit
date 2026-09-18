// Package config manages user-level configuration for jevkit.
package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

const EnvConfigPath = "JEVKIT_CONFIG"

// DefaultDir returns ~/.config/jevkit.
func DefaultDir() string {
	dir, err := os.UserConfigDir()
	if err != nil {
		dir = filepath.Join(os.Getenv("HOME"), ".config")
	}
	return filepath.Join(dir, "jevkit")
}

// DefaultPath returns ~/.config/jevkit/config.yaml.
func DefaultPath() string {
	return filepath.Join(DefaultDir(), "config.yaml")
}

// Path returns the effective config path using CLI override, env override, then default.
func Path(override string) string {
	if path := strings.TrimSpace(override); path != "" {
		return path
	}
	if path := strings.TrimSpace(os.Getenv(EnvConfigPath)); path != "" {
		return path
	}
	return DefaultPath()
}

// Config holds user-level defaults for jevkit.
// All fields are optional — zero values mean "use CLI default".
type Config struct {
	// Add your config fields here, e.g.:
	// OutputFormat string `yaml:"output_format,omitempty"`
	// NoColor      *bool  `yaml:"no_color,omitempty"`
}

// Load reads config from path. Returns empty config (no error) if file is missing.
func Load(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return &Config{}, nil
		}
		return nil, fmt.Errorf("reading config: %w", err)
	}

	var cfg Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("parsing config %s: %w", path, err)
	}
	return &cfg, nil
}

// Save writes config to path, creating parent directories as needed.
func Save(cfg *Config, path string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("creating config directory: %w", err)
	}

	data, err := yaml.Marshal(cfg)
	if err != nil {
		return fmt.Errorf("marshaling config: %w", err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return fmt.Errorf("writing config %s: %w", path, err)
	}
	return nil
}

// BoolPtr returns a pointer to b.
func BoolPtr(b bool) *bool {
	return &b
}

// BoolVal returns the value of a *bool, or the fallback if nil.
func BoolVal(p *bool, fallback bool) bool {
	if p != nil {
		return *p
	}
	return fallback
}
