package main

import "io/fs"

// ownedByUs accepts any owner on Windows; checking the folder's ACL is a follow-up.
func ownedByUs(fs.FileInfo) bool { return true }
