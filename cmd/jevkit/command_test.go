package main

import (
	"bytes"
"path/filepath"
"strings"
	"testing"
)

func executeCommand(t *testing.T, args ...string) string {
	t.Helper()

	var out bytes.Buffer
	rootCmd.SetOut(&out)
	rootCmd.SetErr(&out)
	rootCmd.SetArgs(args)

	configPathOverride = ""
	if flag := rootCmd.PersistentFlags().Lookup("config"); flag != nil {
		_ = flag.Value.Set("")
		flag.Changed = false
	}


	if err := rootCmd.Execute(); err != nil {
		t.Fatalf("jevkit %s failed: %v\n%s", strings.Join(args, " "), err, out.String())
	}
	return out.String()
}

func TestVersionCommandSmoke(t *testing.T) {
	executeCommand(t, "version")
}

func TestVersionAliasSmoke(t *testing.T) {
	executeCommand(t, "v")
}

func TestHelpCommandSmoke(t *testing.T) {
	executeCommand(t, "help")
}

func TestCompletionCommandSmoke(t *testing.T) {
	for _, shell := range []string{"bash", "zsh", "fish", "powershell"} {
		t.Run(shell, func(t *testing.T) {
			executeCommand(t, "completion", shell)
		})
	}
}


func TestConfigPathCommandSmoke(t *testing.T) {
	want := filepath.Join(t.TempDir(), "from-env.yaml")
	t.Setenv("JEVKIT_CONFIG", want)

	out := executeCommand(t, "config", "path")
	if strings.TrimSpace(out) != want {
		t.Fatalf("config path = %q, want %q", strings.TrimSpace(out), want)
	}
}

func TestConfigPathFlagOverride(t *testing.T) {
	envPath := filepath.Join(t.TempDir(), "from-env.yaml")
	flagPath := filepath.Join(t.TempDir(), "from-flag.yaml")
	t.Setenv("JEVKIT_CONFIG", envPath)

	out := executeCommand(t, "--config", flagPath, "config", "path")
	if strings.TrimSpace(out) != flagPath {
		t.Fatalf("config path = %q, want %q", strings.TrimSpace(out), flagPath)
	}
}

