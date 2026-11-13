package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"syscall"
	"testing"
	"time"

	"tailscale.com/envknob"
	"tailscale.com/net/netns"
)

const (
	childEnv       = "NOCTURNE_TAILNET_TEST_CHILD"
	childTargetEnv = "NOCTURNE_TAILNET_TEST_TARGET"
)

// TestMain runs every test with log upload off, as the shipped helper does, and lets a test
// run the helper as a child process to stop it the way Nocturne does. The child's control
// server is a closed loopback port, so it never leaves the machine.
func TestMain(m *testing.M) {
	envknob.SetNoLogsNoSupport()
	if dir := os.Getenv(childEnv); dir != "" {
		netns.SetEnabled(false)
		ctx, stop := signalContext()
		defer stop()
		tg := target{network: "unix", address: filepath.Join(dir, "listener.sock")}
		if t := os.Getenv(childTargetEnv); t != "" {
			tg, _ = parseTarget(t)
		}
		cfg := config{dir: dir, hostname: helperName, target: tg, controlURL: "http://127.0.0.1:1"}
		fmt.Fprintln(os.Stderr, "signals ready")
		os.Exit(run(ctx, cfg, os.Stdin, os.Stdout))
	}
	os.Exit(m.Run())
}

func TestStopsOnSignalAndEndOfInput(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows has no SIGTERM; Nocturne closes input and the job object ends the rest")
	}
	for _, c := range []struct {
		how, target, first string
	}{
		{how: "sigterm", first: stateStarting},
		{how: "end of input", first: stateStarting},
		{how: "sigterm while waiting for the key", target: "http://127.0.0.1:4100"},
	} {
		t.Run(c.how, func(t *testing.T) {
			cmd := exec.Command(os.Args[0], "-test.run=^$")
			cmd.Env = []string{childEnv + "=" + t.TempDir()}
			if c.target != "" {
				cmd.Env = append(cmd.Env, childTargetEnv+"="+c.target)
			}
			stdin, err := cmd.StdinPipe()
			if err != nil {
				t.Fatal(err)
			}
			stdout, err := cmd.StdoutPipe()
			if err != nil {
				t.Fatal(err)
			}
			stderr, err := cmd.StderrPipe()
			if err != nil {
				t.Fatal(err)
			}
			if err := cmd.Start(); err != nil {
				t.Fatal(err)
			}
			ready := make(chan struct{})
			go func() {
				sc := bufio.NewScanner(stderr)
				for sc.Scan() {
					if sc.Text() == "signals ready" {
						close(ready)
					}
				}
			}()
			states := make(chan string, 16)
			go func() {
				defer close(states)
				sc := bufio.NewScanner(stdout)
				for sc.Scan() {
					var ev event
					if json.Unmarshal(sc.Bytes(), &ev) == nil {
						states <- ev.State
					}
				}
			}()
			select {
			case <-ready:
			case <-time.After(stepTimeout):
				cmd.Process.Kill()
				t.Fatal("the child never started")
			}
			if c.first != "" {
				expect(t, states, c.first)
			}
			if strings.HasPrefix(c.how, "sigterm") {
				cmd.Process.Signal(syscall.SIGTERM)
			} else {
				stdin.Close()
			}
			expect(t, states, stateStopped)
			done := make(chan error, 1)
			go func() { done <- cmd.Wait() }()
			select {
			case err := <-done:
				if err != nil {
					t.Fatalf("exit: %v", err)
				}
			case <-time.After(stepTimeout):
				cmd.Process.Kill()
				t.Fatal("the helper did not exit")
			}
		})
	}
}

func expect(t *testing.T, states <-chan string, want string) {
	t.Helper()
	timeout := time.After(stepTimeout)
	for {
		select {
		case got, ok := <-states:
			if !ok {
				t.Fatalf("output ended before %q", want)
			}
			if got == want {
				return
			}
		case <-timeout:
			t.Fatalf("no %q", want)
		}
	}
}

func TestRefusesALoopbackTargetWithoutAKey(t *testing.T) {
	tg, err := parseTarget("http://127.0.0.1:4100")
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range []struct{ input, reason string }{
		{"", reasonKey},
		{"\n", reasonKey},
		{`{"cmd":"status"}` + "\n", reasonKey},
		{`{"key":"short"}` + "\n", reasonKey},
		{`{"key":"has spaces in it, not a token"}` + "\n", reasonKey},
		{`{"key":"` + testKey + `","extra":1}` + "\n", reasonKey},
		{`{"key":"` + testKey + `"} {"cmd":"stop"}` + "\n", reasonKey},
		{`{"key":"` + testKey + `"}x` + "\n", reasonKey},
		{strings.Repeat("k", maxLine+1) + "\n", reasonInput},
	} {
		var out bytes.Buffer
		cfg := config{dir: t.TempDir(), hostname: helperName, target: tg}
		code := run(context.Background(), cfg, strings.NewReader(c.input), &out)
		want := `{"state":"error","reason":"` + c.reason + `"}`
		if code != 1 || strings.TrimSpace(out.String()) != want {
			t.Errorf("input %.40q: code %d, output %q", c.input, code, out.String())
		}
		if entries, _ := os.ReadDir(cfg.dir); len(entries) != 0 {
			t.Errorf("input %.40q: the helper wrote state before it had a key", c.input)
		}
	}
}

func TestOverlongCommandExitsWithAReason(t *testing.T) {
	_, tg := unixOrigin(t)
	var out bytes.Buffer
	cfg := helperConfig(t, "http://127.0.0.1:1", tg)
	code := run(context.Background(), cfg, strings.NewReader(strings.Repeat("x", maxLine+1)+"\n"), &out)
	lines := strings.Split(strings.TrimSpace(out.String()), "\n")
	if code != 1 || !slices.Contains(lines, `{"state":"error","reason":"input"}`) ||
		lines[len(lines)-1] != `{"state":"stopped"}` {
		t.Fatalf("code %d, output %q", code, out.String())
	}
}

func TestParseTarget(t *testing.T) {
	good := map[string]target{
		"unix:/run/n/listener.sock": {network: "unix", address: "/run/n/listener.sock"},
		"http://127.0.0.1:4100":     {network: "tcp", address: "127.0.0.1:4100"},
		"http://127.0.0.1:4100/":    {network: "tcp", address: "127.0.0.1:4100"},
	}
	for in, want := range good {
		got, err := parseTarget(in)
		if err != nil || got != want {
			t.Errorf("%q: got %+v, %v", in, got, err)
		}
	}
	for _, in := range []string{
		"", "unix:", "unix:relative.sock", "https://127.0.0.1:4100", "http://localhost:4100",
		"http://0.0.0.0:4100", "http://192.168.1.2:4100", "http://[::1]:4100", "http://127.0.0.1",
		"http://127.0.0.1:0", "http://127.0.0.1:4100/key", "http://u:p@127.0.0.1:4100",
		"http://127.0.0.1:4100?k=v", "tcp://127.0.0.1:4100",
	} {
		if _, err := parseTarget(in); err == nil {
			t.Errorf("%q was accepted", in)
		}
	}
}

func TestHostnamePattern(t *testing.T) {
	for _, ok := range []string{"nocturne-7f3a", "a", "n0"} {
		if !hostnamePattern.MatchString(ok) {
			t.Errorf("%q refused", ok)
		}
	}
	for _, bad := range []string{"", "-a", "a-", "Nocturne", "a.b", "a b", strings.Repeat("a", 64)} {
		if hostnamePattern.MatchString(bad) {
			t.Errorf("%q accepted", bad)
		}
	}
}

func TestTailscaleEnvironment(t *testing.T) {
	env := []string{"PATH=", "TS_AUTHKEY=x", "HOME=/nowhere", "TSNET_FORCE_LOGIN=1", "ts_debug=1", "TSX=1"}
	got := tailscaleEnvironment(env)
	want := []string{"TS_AUTHKEY", "TSNET_FORCE_LOGIN", "ts_debug"}
	if !slices.Equal(got, want) {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestOwnerOnlyDir(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("permissions are not modes on Windows")
	}
	base := t.TempDir()
	fresh := filepath.Join(base, "remote", "tailscale")
	if err := ownerOnlyDir(fresh); err != nil {
		t.Fatal(err)
	}
	loose := filepath.Join(base, "loose")
	if err := os.Mkdir(loose, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := ownerOnlyDir(loose); err != nil {
		t.Fatal(err)
	}
	for _, dir := range []string{fresh, loose} {
		fi, err := os.Lstat(dir)
		if err != nil || !fi.IsDir() || fi.Mode().Perm() != 0o700 {
			t.Fatalf("%s: %v, %v", dir, fi.Mode(), err)
		}
	}

	link := filepath.Join(base, "link")
	if err := os.Symlink(loose, link); err != nil {
		t.Fatal(err)
	}
	file := filepath.Join(base, "file")
	if err := os.WriteFile(file, nil, 0o600); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []string{link, file} {
		if err := ownerOnlyDir(bad); !errors.Is(err, errUnsafeDir) {
			t.Errorf("%s: %v", bad, err)
		}
	}
	if fi, err := os.Stat("/"); err == nil && os.Getuid() != 0 && ownedByUs(fi) {
		t.Fatal("the root folder looks like ours")
	}
}
