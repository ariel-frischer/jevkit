package main

import (
	"os"

"github.com/demo/jevkit/internal/config"
"github.com/demo/jevkit/internal/version"
	"github.com/fatih/color"
	"github.com/spf13/cobra"
)

var rootCmd = &cobra.Command{
	Use:     "jevkit",
	Short:   "Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting",
	Version: version.Version,
}
var configPathOverride string
func init() {
	// Disable colors when not writing to a terminal.
	if fi, err := os.Stdout.Stat(); err == nil {
		if fi.Mode()&os.ModeCharDevice == 0 {
			color.NoColor = true
		}
	}

	var noColor bool
	rootCmd.PersistentFlags().BoolVar(&noColor, "no-color", false, "disable colored output")
	rootCmd.PersistentFlags().StringVar(&configPathOverride, "config", "", "config file path (default $JEVKIT_CONFIG or ~/.config/jevkit/config.yaml)")
	rootCmd.PersistentPreRun = func(cmd *cobra.Command, args []string) {
		if noColor {
			color.NoColor = true
		}
	}

	rootCmd.SetHelpFunc(colorizedHelp)

	rootCmd.AddCommand(versionCmd)
	rootCmd.AddCommand(configCmd)
}
func selectedConfigPath() string {
	return config.Path(configPathOverride)
}
