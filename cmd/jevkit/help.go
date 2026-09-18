package main

import (
	"fmt"
	"strings"

	"github.com/fatih/color"
	"github.com/spf13/cobra"
)

var (
	helpHeading = color.New(color.FgYellow, color.Bold).SprintFunc()
	helpCmd     = color.New(color.FgCyan).SprintFunc()
	helpFlag    = color.New(color.FgGreen).SprintFunc()
	helpDescFmt = color.New(color.FgWhite).SprintFunc()
	helpDim     = color.New(color.Faint).SprintFunc()
)

func colorizedHelp(cmd *cobra.Command, _ []string) {
	fmt.Printf("%s — %s\n", helpCmd(cmd.CommandPath()), helpDescFmt(cmd.Short))

	if cmd.Long != "" {
		fmt.Println()
		fmt.Println(helpDim(cmd.Long))
	}

	if cmd.Runnable() {
		fmt.Println()
		fmt.Println(helpHeading("Usage:"))
		fmt.Printf("  %s\n", cmd.UseLine())
	}

	if len(cmd.Aliases) > 0 {
		fmt.Println()
		fmt.Println(helpHeading("Aliases:"))
		fmt.Printf("  %s\n", strings.Join(cmd.Aliases, ", "))
	}

	if cmds := visibleSubcommands(cmd); len(cmds) > 0 {
		fmt.Println()
		fmt.Println(helpHeading("Commands:"))
		maxLen := maxCommandNameLen(cmds)
		for _, sub := range cmds {
			padding := strings.Repeat(" ", maxLen-len(sub.Name())+2)
			fmt.Printf("  %s%s%s\n", helpCmd(sub.Name()), padding, helpDim(sub.Short))
		}
	}

	if flags := cmd.LocalFlags().FlagUsages(); flags != "" {
		fmt.Println()
		fmt.Println(helpHeading("Flags:"))
		printColorizedFlags(flags)
	}

	if flags := cmd.InheritedFlags().FlagUsages(); flags != "" {
		fmt.Println()
		fmt.Println(helpHeading("Global Flags:"))
		printColorizedFlags(flags)
	}

	if cmd.HasAvailableSubCommands() {
		fmt.Println()
		fmt.Printf("%s %s %s\n",
			helpDim("Use"),
			helpCmd(fmt.Sprintf("%s [command] --help", cmd.CommandPath())),
			helpDim("for more information about a command."),
		)
	}

	fmt.Println()
}

func printColorizedFlags(flagUsages string) {
	for _, line := range strings.Split(strings.TrimRight(flagUsages, "\n"), "\n") {
		if line == "" {
			fmt.Println()
			continue
		}

		trimmed := strings.TrimLeft(line, " ")
		indent := len(line) - len(trimmed)

		parts := splitFlagLine(trimmed)
		if len(parts) == 2 {
			fmt.Printf("%s%s  %s\n",
				strings.Repeat(" ", indent),
				helpFlag(parts[0]),
				helpDim(parts[1]),
			)
		} else {
			fmt.Printf("%s%s\n", strings.Repeat(" ", indent), helpFlag(trimmed))
		}
	}
}

func splitFlagLine(s string) []string {
	for i := 0; i < len(s)-2; i++ {
		if s[i] != ' ' && s[i+1] == ' ' && s[i+2] == ' ' && i+3 < len(s) {
			desc := strings.TrimLeft(s[i+1:], " ")
			if desc != "" {
				return []string{s[:i+1], desc}
			}
		}
	}
	return []string{s}
}

func visibleSubcommands(cmd *cobra.Command) []*cobra.Command {
	var cmds []*cobra.Command
	for _, sub := range cmd.Commands() {
		if !sub.Hidden && sub.Name() != "help" {
			cmds = append(cmds, sub)
		}
	}
	return cmds
}

func maxCommandNameLen(cmds []*cobra.Command) int {
	max := 0
	for _, c := range cmds {
		if len(c.Name()) > max {
			max = len(c.Name())
		}
	}
	return max
}
