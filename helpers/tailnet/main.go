// Command nocturne-tailnet puts Nocturne's local listener on the person's tailnet and nowhere
// else: one TLS listener on port 443 of this node's tailnet addresses, proxied to the listener,
// for the person's own untagged devices only. Any other port, TCP or UDP, is refused; nothing is
// forwarded to the computer's loopback.
//
// Nocturne talks to it in JSON lines. On stdout, events:
//
//	{"state":"starting"}
//	{"state":"needsLogin","authUrl":"..."}
//	{"state":"needsApproval"}
//	{"state":"needsHttps"}
//	{"state":"running","host":"<name>.<tailnet>.ts.net"}
//	{"state":"error","reason":"key|input|start|backend|blocked|login|listen|certificate|logout"}
//	{"state":"stopped"}
//
// On stdin, commands: {"cmd":"status"} repeats the current state, {"cmd":"logout"} signs out
// and exits, {"cmd":"stop"} exits. With an http:// target the first line must be
// {"key":"..."}. The end of stdin or SIGTERM also exits; a line over 4 KB exits with an error.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"io/fs"
	"log"
	"os"
	"os/signal"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"syscall"

	"tailscale.com/envknob"
)

var hostnamePattern = regexp.MustCompile(`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`)

func main() {
	os.Exit(mainCode())
}

func mainCode() int {
	fs := flag.NewFlagSet("nocturne-tailnet", flag.ContinueOnError)
	stateDir := fs.String("state-dir", "", "owner-only folder for the node's state (absolute)")
	hostname := fs.String("hostname", "", "machine name on the tailnet")
	targetFlag := fs.String("target", "", "unix:<absolute path> or http://127.0.0.1:<port>")
	if err := fs.Parse(os.Args[1:]); err != nil {
		return 2
	}
	if fs.NArg() != 0 || *stateDir == "" || !filepath.IsAbs(*stateDir) ||
		!hostnamePattern.MatchString(*hostname) {
		fmt.Fprintln(os.Stderr, "usage: nocturne-tailnet --state-dir <abs> --hostname <name> --target <target>")
		return 2
	}
	t, err := parseTarget(*targetFlag)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	if missing := omittedFeaturesPresent(); len(missing) > 0 {
		fmt.Fprintf(os.Stderr, "built without the release tags (%s); use scripts/build-tailnet.sh\n",
			strings.Join(missing, ", "))
		return 2
	}
	if names := tailscaleEnvironment(os.Environ()); len(names) > 0 {
		fmt.Fprintf(os.Stderr, "refusing to start with Tailscale settings in the environment: %s\n",
			strings.Join(names, ", "))
		return 2
	}
	if err := ownerOnlyDir(*stateDir); err != nil {
		fmt.Fprintln(os.Stderr, "state folder:", err)
		return 2
	}

	envknob.SetNoLogsNoSupport()
	log.SetOutput(io.Discard)

	ctx, stop := signalContext()
	defer stop()
	return run(ctx, config{dir: *stateDir, hostname: *hostname, target: t}, os.Stdin, os.Stdout)
}

func signalContext() (context.Context, context.CancelFunc) {
	return signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
}

// tailscaleEnvironment names the Tailscale knobs in env. Nocturne starts the helper with a
// scrubbed environment, so any of them means something else is steering it.
func tailscaleEnvironment(env []string) []string {
	var names []string
	for _, kv := range env {
		name, _, _ := strings.Cut(kv, "=")
		upper := strings.ToUpper(name)
		if strings.HasPrefix(upper, "TS_") || strings.HasPrefix(upper, "TSNET_") {
			names = append(names, name)
		}
	}
	return names
}

var errUnsafeDir = errors.New("not a folder of ours: a link, another owner, or not a folder")

// ownerOnlyDir makes dir, or accepts it only as a real folder (not a link) owned by this user,
// and sets it to 0700. On Windows the folder keeps its inherited ACL (a follow-up).
func ownerOnlyDir(dir string) error {
	fi, err := os.Lstat(dir)
	if errors.Is(err, fs.ErrNotExist) {
		if err := os.MkdirAll(filepath.Dir(dir), 0o700); err != nil {
			return err
		}
		if err := os.Mkdir(dir, 0o700); err != nil {
			return err
		}
		fi, err = os.Lstat(dir)
	}
	if err != nil {
		return err
	}
	if fi.Mode()&fs.ModeSymlink != 0 || !fi.IsDir() || !ownedByUs(fi) {
		return errUnsafeDir
	}
	if runtime.GOOS == "windows" || fi.Mode().Perm() == 0o700 {
		return nil
	}
	return os.Chmod(dir, 0o700)
}
