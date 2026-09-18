package main

import (
	"fmt"
	"runtime"
	"strings"

	"github.com/demo/jevkit/internal/version"
	"github.com/fatih/color"
	"github.com/spf13/cobra"
)

var versionPlain bool

var versionCmd = &cobra.Command{
	Use:     "version",
	Aliases: []string{"v"},
	Short:   "Display version information",
	Run: func(cmd *cobra.Command, args []string) {
		if versionPlain {
			printPlainVersion()
		} else {
			printPrettyVersion()
		}
	},
}

func init() {
	versionCmd.Flags().BoolVar(&versionPlain, "plain", false, "Plain output without formatting")
}

func printPlainVersion() {
	fmt.Printf("jevkit %s\n", version.Version)
	fmt.Printf("commit: %s\n", version.Commit)
	fmt.Printf("built: %s\n", version.BuildDate)
	fmt.Printf("go: %s\n", runtime.Version())
	fmt.Printf("platform: %s/%s\n", runtime.GOOS, runtime.GOARCH)
}

func printPrettyVersion() {
	dim := color.New(color.Faint).SprintFunc()
	white := color.New(color.FgWhite, color.Bold).SprintFunc()
	yellow := color.New(color.FgYellow).SprintFunc()

	fmt.Println()
	fmt.Println(dim("  jevkit — Fast Rust CLI for TypeSafe Jev: typed decisions, offline linting"))
	fmt.Println()

	info := []struct {
		label string
		value string
	}{
		{"Version", version.Version},
		{"Commit", truncateCommit(version.Commit)},
		{"Built", version.BuildDate},
		{"Go", runtime.Version()},
		{"Platform", fmt.Sprintf("%s/%s", runtime.GOOS, runtime.GOARCH)},
	}

	boxWidth := 44
	topLeft, topRight := "╭", "╮"
	bottomLeft, bottomRight := "╰", "╯"
	horizontal, vertical := "─", "│"

	fmt.Println(topLeft + strings.Repeat(horizontal, boxWidth-2) + topRight)
	fmt.Println(vertical + strings.Repeat(" ", boxWidth-2) + vertical)
	for _, item := range info {
		label := yellow(fmt.Sprintf("%10s", item.label))
		value := white(item.value)
		contentLen := 2 + 2 + 10 + 2 + len(item.value)
		padding := boxWidth - 2 - contentLen
		if padding < 0 {
			padding = 0
		}
		fmt.Println(vertical + " " + fmt.Sprintf("  %s  %s", label, value) + strings.Repeat(" ", padding) + " " + vertical)
	}
	fmt.Println(vertical + strings.Repeat(" ", boxWidth-2) + vertical)
	fmt.Println(bottomLeft + strings.Repeat(horizontal, boxWidth-2) + bottomRight)
	fmt.Println()
}

func truncateCommit(commit string) string {
	if len(commit) > 8 {
		return commit[:8]
	}
	return commit
}
