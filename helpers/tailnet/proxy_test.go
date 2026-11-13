package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"tailscale.com/client/tailscale/apitype"
	"tailscale.com/tailcfg"
)

func TestOwnDevice(t *testing.T) {
	const me, other tailcfg.UserID = 7, 8
	who := func(change func(*apitype.WhoIsResponse)) *apitype.WhoIsResponse {
		w := &apitype.WhoIsResponse{
			Node:        &tailcfg.Node{User: me},
			UserProfile: &tailcfg.UserProfile{ID: me},
		}
		change(w)
		return w
	}
	if !ownDevice(who(func(*apitype.WhoIsResponse) {}), me) {
		t.Fatal("the person's own device was refused")
	}
	for name, w := range map[string]*apitype.WhoIsResponse{
		"nothing":        nil,
		"no node":        who(func(w *apitype.WhoIsResponse) { w.Node = nil }),
		"no profile":     who(func(w *apitype.WhoIsResponse) { w.UserProfile = nil }),
		"another user":   who(func(w *apitype.WhoIsResponse) { w.Node.User, w.UserProfile.ID = other, other }),
		"mixed identity": who(func(w *apitype.WhoIsResponse) { w.UserProfile.ID = other }),
		"tagged":         who(func(w *apitype.WhoIsResponse) { w.Node.Tags = []string{"tag:x"} }),
		"shared in":      who(func(w *apitype.WhoIsResponse) { w.Node.Sharer = other }),
	} {
		if ownDevice(w, me) {
			t.Errorf("%s was allowed", name)
		}
	}
	if ownDevice(who(func(*apitype.WhoIsResponse) {}), 0) {
		t.Error("allowed with no user of our own")
	}
}

func TestSameHost(t *testing.T) {
	const host = "nocturne-ab12.tail-scale.ts.net"
	for _, ok := range []string{host, host + ":443", "NOCTURNE-AB12.tail-scale.ts.net", host + "."} {
		if !sameHost(ok, host) {
			t.Errorf("%q refused", ok)
		}
	}
	for _, bad := range []string{"", "100.64.0.1", host + ":8443", "x." + host, host + ".evil.example"} {
		if sameHost(bad, host) {
			t.Errorf("%q accepted", bad)
		}
	}
	if sameHost(host, "") {
		t.Error("accepted with no name of our own")
	}
}

func TestUpgradeAllowed(t *testing.T) {
	for _, c := range []struct {
		values []string
		ok     bool
	}{
		{nil, true},
		{[]string{"websocket"}, true},
		{[]string{"WebSocket"}, true},
		{[]string{"h2c"}, false},
		{[]string{"websocket", "h2c"}, false},
		{[]string{"websocket, h2c"}, false},
	} {
		h := http.Header{}
		for _, v := range c.values {
			h.Add("Upgrade", v)
		}
		if got := upgradeAllowed(h); got != c.ok {
			t.Errorf("%q: %v", c.values, got)
		}
	}
}

func TestStripKey(t *testing.T) {
	h := http.Header{
		keyHeader:                      {"a"},
		"Nocturne_helper_key":          {"b"},
		http.TrailerPrefix + keyHeader: {"c"},
		"Trailer":                      {"X-One, " + keyHeader, "nocturne_helper_key"},
		"X-One":                        {"kept"},
	}
	if !hasKey(h) {
		t.Fatal("the key was not found")
	}
	stripKey(h)
	if hasKey(h) || h.Get("X-One") != "kept" || h.Get("Trailer") != "X-One" {
		t.Fatalf("after stripping: %v", h)
	}
	h = http.Header{"Trailer": {keyHeader}}
	stripKey(h)
	if _, ok := h["Trailer"]; ok {
		t.Fatalf("an empty announcement was left: %v", h)
	}
}

// testSelf is the node's user in the listener tests.
const testSelf tailcfg.UserID = 7

// ourClients holds the local addresses of connections the tests dialled. Other programs on a
// shared machine sometimes connect to loopback ports; the listeners here refuse them unasked.
var ourClients sync.Map

// loopbackPeers runs a peerListener on loopback; allow decides each check of a test's own
// connection.
func loopbackPeers(t *testing.T, allow allowFunc, pending, conns int) *peerListener {
	t.Helper()
	inner, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	ours := func(ctx context.Context, remoteAddr string, self tailcfg.UserID) bool {
		for deadline := time.Now().Add(2 * time.Second); time.Now().Before(deadline); {
			if _, ok := ourClients.Load(remoteAddr); ok {
				return allow(ctx, remoteAddr, self)
			}
			time.Sleep(5 * time.Millisecond)
		}
		return false
	}
	l := newPeerListener(inner, ours, testSelf, pending, conns)
	t.Cleanup(func() { l.Close() })
	return l
}

func dial(t *testing.T, l *peerListener) net.Conn {
	t.Helper()
	c, err := net.Dial("tcp", l.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	ourClients.Store(c.LocalAddr().String(), true)
	t.Cleanup(func() { c.Close() })
	return c
}

// closedSoon reports whether the helper side closed c within d.
func closedSoon(c net.Conn, d time.Duration) bool {
	c.SetReadDeadline(time.Now().Add(d))
	_, err := c.Read(make([]byte, 1))
	var ne net.Error
	return err != nil && !(errors.As(err, &ne) && ne.Timeout())
}

func accepted(l *peerListener, d time.Duration) (net.Conn, bool) {
	got := make(chan net.Conn, 1)
	go func() {
		if c, err := l.Accept(); err == nil {
			got <- c
		}
	}()
	select {
	case c := <-got:
		return c, true
	case <-time.After(d):
		return nil, false
	}
}

func TestRefusedPeersAreClosedAndHoldNoSlot(t *testing.T) {
	var allowed atomic.Bool
	l := loopbackPeers(t, func(context.Context, string, tailcfg.UserID) bool { return allowed.Load() }, maxPending, 1)
	for range 5 {
		if c := dial(t, l); !closedSoon(c, 5*time.Second) {
			t.Fatal("a refused peer's connection stayed open")
		}
	}
	allowed.Store(true)
	dial(t, l)
	if _, ok := accepted(l, 5*time.Second); !ok {
		t.Fatal("refused peers used up the only slot")
	}
	second := dial(t, l)
	if !closedSoon(second, 5*time.Second) {
		t.Fatal("a connection beyond the limit stayed open")
	}
}

func TestOnlyAFewWaitForTheCheck(t *testing.T) {
	release := make(chan struct{})
	var asked atomic.Int32
	l := loopbackPeers(t, func(ctx context.Context, _ string, _ tailcfg.UserID) bool {
		asked.Add(1)
		select {
		case <-release:
		case <-ctx.Done():
		}
		return true
	}, 2, 8)
	waiting := []net.Conn{dial(t, l), dial(t, l)}
	deadline := time.Now().Add(5 * time.Second)
	for asked.Load() < 2 && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if extra := dial(t, l); !closedSoon(extra, 5*time.Second) {
		t.Fatal("a third connection waited while two were being checked")
	}
	if asked.Load() != 2 {
		t.Fatalf("%d checks, want 2", asked.Load())
	}
	close(release)
	for range waiting {
		if _, ok := accepted(l, 5*time.Second); !ok {
			t.Fatal("a checked connection was not accepted")
		}
	}
}

func TestRevalidateClosesConnectionsNoLongerAllowed(t *testing.T) {
	var allowed atomic.Bool
	allowed.Store(true)
	l := loopbackPeers(t, func(context.Context, string, tailcfg.UserID) bool { return allowed.Load() }, maxPending, maxConns)
	client := dial(t, l)
	server, ok := accepted(l, 5*time.Second)
	if !ok {
		t.Fatal("not accepted")
	}
	defer server.Close()
	l.revalidate(context.Background())
	if closedSoon(client, 300*time.Millisecond) {
		t.Fatal("an allowed connection was closed")
	}
	allowed.Store(false)
	l.revalidate(context.Background())
	if !closedSoon(client, 5*time.Second) {
		t.Fatal("a connection whose peer lost access stayed open")
	}
	held := func() (int, int) {
		l.mu.Lock()
		defer l.mu.Unlock()
		return len(l.open), len(l.slots)
	}
	deadline := time.Now().Add(5 * time.Second)
	for open, slots := held(); open+slots > 0; open, slots = held() {
		if time.Now().After(deadline) {
			t.Fatalf("%d open, %d slots held after closing", open, slots)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

// TestEachRequestRechecksThePeer: on one kept-alive connection, a request after the cache
// window asks again and is refused once the peer lost access.
func TestEachRequestRechecksThePeer(t *testing.T) {
	var allowed atomic.Bool
	var asked atomic.Int32
	allowed.Store(true)
	l := loopbackPeers(t, func(context.Context, string, tailcfg.UserID) bool {
		asked.Add(1)
		return allowed.Load()
	}, maxPending, maxConns)
	const host = "nocturne-ab12.tail-scale.ts.net"
	srv := newHTTPServer(&gate{
		host:    host,
		recheck: 200 * time.Millisecond,
		next:    http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(204) }),
	})
	go srv.Serve(l)
	t.Cleanup(func() { srv.Close() })
	conns := 0
	client := &http.Client{Transport: &http.Transport{
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			conns++
			c, err := (&net.Dialer{}).DialContext(ctx, "tcp", l.Addr().String())
			if err == nil {
				ourClients.Store(c.LocalAddr().String(), true)
			}
			return c, err
		},
	}}
	do := func() (int, error) {
		res, err := client.Get("http://" + host + "/s/x")
		if err != nil {
			return 0, err
		}
		res.Body.Close()
		return res.StatusCode, nil
	}
	if code, err := do(); err != nil || code != 204 {
		t.Fatalf("first request: %d, %v", code, err)
	}
	if code, err := do(); err != nil || code != 204 {
		t.Fatalf("second request: %d, %v", code, err)
	}
	if asked.Load() != 1 || conns != 1 {
		t.Fatalf("asked %d times on %d connections, want once on one", asked.Load(), conns)
	}
	allowed.Store(false)
	time.Sleep(300 * time.Millisecond)
	if code, err := do(); err == nil && code != http.StatusForbidden {
		t.Fatalf("after losing access: %d", code)
	}
}

func TestCloseAllClosesEveryConnection(t *testing.T) {
	l := loopbackPeers(t, func(context.Context, string, tailcfg.UserID) bool { return true }, maxPending, maxConns)
	var clients []net.Conn
	for range 3 {
		clients = append(clients, dial(t, l))
		if _, ok := accepted(l, 5*time.Second); !ok {
			t.Fatal("not accepted")
		}
	}
	l.closeAll()
	for i, c := range clients {
		if !closedSoon(c, 5*time.Second) {
			t.Fatalf("connection %d stayed open", i)
		}
	}
}

// TestACheckFromBeforeAnAccountChangeIsRefused: a connection whose check passed under the old
// account, and which reaches admission after the change, is closed, not served.
func TestACheckFromBeforeAnAccountChangeIsRefused(t *testing.T) {
	var asked []tailcfg.UserID
	var mu sync.Mutex
	l := loopbackPeers(t, func(_ context.Context, _ string, self tailcfg.UserID) bool {
		mu.Lock()
		asked = append(asked, self)
		mu.Unlock()
		return self == testSelf
	}, maxPending, maxConns)
	changed := make(chan struct{})
	var once sync.Once
	l.mu.Lock()
	l.afterAllow = func() {
		once.Do(func() {
			l.setSelf(testSelf + 1)
			close(changed)
		})
	}
	l.mu.Unlock()
	c := dial(t, l)
	<-changed
	if !closedSoon(c, 5*time.Second) {
		t.Fatal("a connection checked under the old account was admitted")
	}
	if _, ok := accepted(l, 300*time.Millisecond); ok {
		t.Fatal("a connection checked under the old account was accepted")
	}
	mu.Lock()
	defer mu.Unlock()
	if len(asked) != 1 || asked[0] != testSelf {
		t.Fatalf("checked as %v", asked)
	}
	if open, slots := len(l.snapshot()), len(l.slots); open+slots != 0 {
		t.Fatalf("%d open, %d slots held", open, slots)
	}
}

// TestAnAccountChangeRefusesEvenAFreshCheck: once the node's user differs from the one a
// connection was admitted for, its next request is refused however recent its last check.
func TestAnAccountChangeRefusesEvenAFreshCheck(t *testing.T) {
	var asked atomic.Int32
	l := loopbackPeers(t, func(context.Context, string, tailcfg.UserID) bool {
		asked.Add(1)
		return true
	}, maxPending, maxConns)
	dial(t, l)
	c, ok := accepted(l, 5*time.Second)
	if !ok {
		t.Fatal("not accepted")
	}
	ac := c.(*admittedConn)
	if !ac.recheck(context.Background(), time.Hour) {
		t.Fatal("refused before any change")
	}
	l.mu.Lock()
	l.self = testSelf + 1
	l.mu.Unlock()
	if ac.recheck(context.Background(), time.Hour) {
		t.Fatal("served for the old account within the cache window")
	}
	if asked.Load() != 1 {
		t.Fatalf("asked %d times, want only at admission", asked.Load())
	}
}

// TestWatchPeersFailsClosedAndRetries: when the account cannot be read, every connection is
// closed and no one is allowed, and the read is retried until it works.
func TestWatchPeersFailsClosedAndRetries(t *testing.T) {
	l := loopbackPeers(t, func(_ context.Context, _ string, self tailcfg.UserID) bool {
		return self == testSelf
	}, maxPending, maxConns)
	client := dial(t, l)
	if _, ok := accepted(l, 5*time.Second); !ok {
		t.Fatal("not accepted")
	}
	var reads atomic.Int32
	failing := make(chan struct{})
	selfUser := func(context.Context) (tailcfg.UserID, error) {
		switch reads.Add(1) {
		case 1:
			close(failing)
			return 0, errors.New("status unavailable")
		case 2:
			return 0, errors.New("status unavailable")
		}
		return testSelf, nil
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	wake := make(chan struct{}, 1)
	go watchPeers(ctx, l, selfUser, wake, 10*time.Millisecond)
	wake <- struct{}{}
	<-failing
	if !closedSoon(client, 5*time.Second) {
		t.Fatal("a connection stayed open while the account could not be read")
	}
	deadline := time.Now().Add(5 * time.Second)
	for {
		if self, _ := l.identity(); self == testSelf && reads.Load() >= 3 {
			break
		}
		if time.Now().After(deadline) {
			self, _ := l.identity()
			t.Fatalf("after %d reads the account is %v", reads.Load(), self)
		}
		time.Sleep(10 * time.Millisecond)
	}
	dial(t, l)
	if _, ok := accepted(l, 5*time.Second); !ok {
		t.Fatal("not accepted once the account read again")
	}
}
