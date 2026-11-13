//go:build !windows

package main

import (
	"io/fs"
	"os"
	"syscall"
)

func ownedByUs(fi fs.FileInfo) bool {
	st, ok := fi.Sys().(*syscall.Stat_t)
	return ok && int(st.Uid) == os.Getuid()
}
