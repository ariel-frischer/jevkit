package main

import (
	"fmt"

	"github.com/fatih/color"
)

var (
	successFmt = color.New(color.FgGreen)
	warnFmt    = color.New(color.FgYellow)
	errFmt     = color.New(color.FgRed)
	boldFmt    = color.New(color.Bold)
	fileFmt    = color.New(color.FgCyan)
	versionFmt = color.New(color.FgMagenta, color.Bold)
)

// success prints a green success message.
func success(format string, a ...any) {
	fmt.Println(successFmt.Sprintf(format, a...))
}

// warn prints a yellow warning message.
func warn(format string, a ...any) {
	fmt.Println(warnFmt.Sprintf(format, a...))
}

// highlight returns a bold string.
func highlight(s string) string {
	return boldFmt.Sprint(s)
}

// fileRef returns a cyan-colored file path.
func fileRef(s string) string {
	return fileFmt.Sprint(s)
}

// versionRef returns a magenta bold version string.
func versionRef(s string) string {
	return versionFmt.Sprint(s)
}
