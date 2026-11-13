package main

import (
	"bufio"
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha1"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/http"
	"net/http/httptest"
	"net/http/httptrace"
	"net/netip"
	"net/textproto"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"tailscale.com/derp/derpserver"
	"tailscale.com/ipn/store/mem"
	"tailscale.com/net/netns"
	"tailscale.com/net/stun/stuntest"
	"tailscale.com/tailcfg"
	"tailscale.com/tailcfg/nodecap"
	"tailscale.com/tsnet"
	"tailscale.com/tstest"
	"tailscale.com/tstest/integration/testcontrol"
	"tailscale.com/types/key"
	"tailscale.com/types/logger"
	"tailscale.com/types/nettype"
)

const (
	magicDomain  = "tail-scale.ts.net"
	helperName   = "nocturne-ab12"
	testKey      = "k3y-0123456789abcdef-k3y"
	stepTimeout  = 90 * time.Second
	refuseWithin = 5 * time.Second
)

type controlOpts struct {
	https       bool
	requireAuth bool
	// otherUsers gives every node its own user; by default all nodes are the person's own.
	otherUsers bool
}

// startControl runs Tailscale's in-process control plane and DERP relay on loopback.
func startControl(t *testing.T, o controlOpts) (*testcontrol.Server, string) {
	t.Helper()
	netns.SetEnabled(false)
	t.Cleanup(func() { netns.SetEnabled(true) })
	control := &testcontrol.Server{
		DERPMap:          runDERP(t),
		RequireAuth:      o.requireAuth,
		AllNodesSameUser: !o.otherUsers,
		Logf:             logger.Discard,
	}
	if o.https {
		control.DNSConfig = &tailcfg.DNSConfig{Proxied: true}
		control.MagicDNSDomain = magicDomain
	}
	// testcontrol panics on a path it does not know. Other processes on a shared machine probe
	// loopback ports with GET /, so that path gets a plain 404 instead.
	control.HTTPTestServer = httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/" {
			http.NotFound(w, r)
			return
		}
		control.ServeHTTP(w, r)
	}))
	control.HTTPTestServer.Start()
	t.Cleanup(control.HTTPTestServer.Close)
	return control, control.HTTPTestServer.URL
}

// runDERP runs a DERP relay and a STUN server on loopback, as Tailscale's integration tests
// do, without that package (it links features the release build leaves out).
func runDERP(t *testing.T) *tailcfg.DERPMap {
	t.Helper()
	d := derpserver.New(key.NewNode(), logger.Discard)
	srv := httptest.NewUnstartedServer(derpserver.Handler(d))
	srv.Config.ErrorLog = discardLogger()
	srv.Config.TLSNextProto = map[string]func(*http.Server, *tls.Conn, http.Handler){}
	srv.StartTLS()
	stunAddr, stunCleanup := stuntest.ServeWithPacketListener(t, nettype.Std{})
	t.Cleanup(func() {
		srv.CloseClientConnections()
		srv.Close()
		d.Close()
		stunCleanup()
	})
	return &tailcfg.DERPMap{Regions: map[tailcfg.DERPRegionID]*tailcfg.DERPRegion{
		1: {RegionID: 1, RegionCode: "test", Nodes: []*tailcfg.DERPNode{{
			Name:             "t1",
			RegionID:         1,
			HostName:         "127.0.0.1",
			IPv4:             "127.0.0.1",
			IPv6:             "none",
			STUNPort:         stunAddr.Port,
			DERPPort:         srv.Listener.Addr().(*net.TCPAddr).Port,
			InsecureForTests: true,
			STUNTestIP:       "127.0.0.1",
		}}},
	}}
}

// testCA issues the helper's certificate. Tailscale accepts a cached certificate from an
// unknown authority only when its ACME directory is not the default, so tests point that
// at a closed loopback port; nothing leaves the machine.
type testCA struct {
	cert *x509.Certificate
	key  *ecdsa.PrivateKey
	pool *x509.CertPool
}

func newTestCA(t *testing.T) *testCA {
	t.Helper()
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	tmpl := &x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "test root"},
		NotBefore:             time.Now().Add(-time.Hour),
		NotAfter:              time.Now().Add(365 * 24 * time.Hour),
		IsCA:                  true,
		KeyUsage:              x509.KeyUsageCertSign,
		BasicConstraintsValid: true,
	}
	der, err := x509.CreateCertificate(rand.Reader, tmpl, tmpl, &key.PublicKey, key)
	if err != nil {
		t.Fatal(err)
	}
	cert, err := x509.ParseCertificate(der)
	if err != nil {
		t.Fatal(err)
	}
	pool := x509.NewCertPool()
	pool.AddCert(cert)
	return &testCA{cert: cert, key: key, pool: pool}
}

func (ca *testCA) placeCert(t *testing.T, stateDir, domain string) {
	t.Helper()
	t.Setenv("TS_DEBUG_ACME_DIRECTORY_URL", "http://127.0.0.1:1/directory")
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	tmpl := &x509.Certificate{
		SerialNumber: big.NewInt(2),
		DNSNames:     []string{domain},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(300 * 24 * time.Hour),
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	der, err := x509.CreateCertificate(rand.Reader, tmpl, ca.cert, &key.PublicKey, ca.key)
	if err != nil {
		t.Fatal(err)
	}
	keyDER, err := x509.MarshalPKCS8PrivateKey(key)
	if err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(stateDir, "certs")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	certPEM := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: der})
	keyPEM := pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: keyDER})
	if err := os.WriteFile(filepath.Join(dir, domain+".crt"), certPEM, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, domain+".key"), keyPEM, 0o600); err != nil {
		t.Fatal(err)
	}
}

// helperRun is the helper running in this process, as Nocturne would see it.
type helperRun struct {
	stdin  *io.PipeWriter
	events chan event
	done   chan int

	mu    sync.Mutex
	lines []string
}

func startHelper(t *testing.T, ctx context.Context, cfg config, firstLine string) *helperRun {
	t.Helper()
	inR, inW := io.Pipe()
	outR, outW := io.Pipe()
	h := &helperRun{stdin: inW, events: make(chan event, 64), done: make(chan int, 1)}
	go func() {
		code := run(ctx, cfg, inR, outW)
		outW.Close()
		h.done <- code
	}()
	go func() {
		defer close(h.events)
		sc := bufio.NewScanner(outR)
		for sc.Scan() {
			h.mu.Lock()
			h.lines = append(h.lines, sc.Text())
			h.mu.Unlock()
			var ev event
			if err := json.Unmarshal(sc.Bytes(), &ev); err != nil {
				t.Errorf("not a JSON line: %q", sc.Text())
				continue
			}
			h.events <- ev
		}
	}()
	if firstLine != "" {
		h.send(t, firstLine)
	}
	t.Cleanup(func() {
		inW.Close()
		select {
		case <-h.done:
		case <-time.After(stepTimeout):
			t.Error("the helper did not stop")
		}
	})
	return h
}

func (h *helperRun) send(t *testing.T, line string) {
	t.Helper()
	if _, err := io.WriteString(h.stdin, line+"\n"); err != nil {
		t.Fatalf("writing %q: %v", line, err)
	}
}

// waitFor returns the first event in state, failing on an error event first.
func (h *helperRun) waitFor(t *testing.T, state string) event {
	t.Helper()
	timeout := time.After(stepTimeout)
	for {
		select {
		case ev, ok := <-h.events:
			if !ok {
				t.Fatalf("the helper stopped before %q", state)
			}
			if ev.State == state {
				return ev
			}
			if ev.State == stateError {
				t.Fatalf("error before %q: %+v", state, ev)
			}
		case <-timeout:
			t.Fatalf("no %q within %v", state, stepTimeout)
		}
	}
}

func (h *helperRun) exitCode(t *testing.T) int {
	t.Helper()
	select {
	case code := <-h.done:
		h.done <- code
		return code
	case <-time.After(stepTimeout):
		t.Fatal("the helper did not exit")
		return -1
	}
}

func (h *helperRun) output() string {
	h.mu.Lock()
	defer h.mu.Unlock()
	return strings.Join(h.lines, "\n")
}

// startPhone joins the tailnet as a plain tsnet node, standing in for the phone.
func startPhone(t *testing.T, ctx context.Context, controlURL, name string) *tsnet.Server {
	t.Helper()
	s := &tsnet.Server{
		Dir:        filepath.Join(t.TempDir(), name),
		ControlURL: controlURL,
		Hostname:   name,
		Store:      new(mem.Store),
		Ephemeral:  true,
		UserLogf:   logger.Discard,
	}
	t.Cleanup(func() { s.Close() })
	if _, err := s.Up(ctx); err != nil {
		t.Fatal(err)
	}
	return s
}

// helperAddrs returns the helper's IPv4 and IPv6 tailnet addresses as the phone sees them.
func helperAddrs(t *testing.T, ctx context.Context, phone *tsnet.Server) (v4, v6 netip.Addr) {
	t.Helper()
	lc, err := phone.LocalClient()
	if err != nil {
		t.Fatal(err)
	}
	err = tstest.WaitFor(stepTimeout, func() error {
		st, err := lc.Status(ctx)
		if err != nil {
			return err
		}
		for _, p := range st.Peer {
			if p.HostName != helperName {
				continue
			}
			for _, ip := range p.TailscaleIPs {
				if ip.Is4() {
					v4 = ip
				} else {
					v6 = ip
				}
			}
			if v4.IsValid() && v6.IsValid() {
				return nil
			}
		}
		return errors.New("the phone does not see the helper yet")
	})
	if err != nil {
		t.Fatal(err)
	}
	return v4, v6
}

// phoneClient talks HTTPS to the helper at ip:443, one connection per request.
func phoneClient(phone *tsnet.Server, ip netip.Addr, pool *x509.CertPool) *http.Client {
	return &http.Client{
		Timeout: 20 * time.Second,
		Transport: &http.Transport{
			DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
				return phone.Dial(ctx, "tcp", netip.AddrPortFrom(ip, 443).String())
			},
			TLSClientConfig:   &tls.Config{RootCAs: pool},
			DisableKeepAlives: true,
		},
	}
}

// doWithRetry retries while the two nodes find each other through the relay.
func doWithRetry(t *testing.T, c *http.Client, req func() *http.Request) *http.Response {
	t.Helper()
	var res *http.Response
	err := tstest.WaitFor(stepTimeout, func() error {
		r, err := c.Do(req())
		if err != nil {
			return err
		}
		res = r
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return res
}

func status(t *testing.T, c *http.Client, req *http.Request) (int, http.Header, string) {
	t.Helper()
	res, err := c.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	body, _ := io.ReadAll(res.Body)
	return res.StatusCode, res.Header, string(body)
}

func get(t *testing.T, url string) *http.Request {
	t.Helper()
	r, err := http.NewRequest("GET", url, nil)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

// origin stands in for Nocturne's listener and records what reaches it. It also tries to send
// the key back in every header block, as a careless listener might.
type origin struct {
	mu   sync.Mutex
	seen []*http.Request
	key  []string
}

func (o *origin) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	o.mu.Lock()
	o.seen = append(o.seen, r)
	o.key = append(o.key, r.Header.Values(keyHeader)...)
	o.mu.Unlock()
	switch r.URL.Path {
	case "/ws":
		o.upgrade(w, r)
		return
	case "/early":
		w.Header().Set("Link", "</s/app.js>; rel=preload")
		w.Header().Set(keyHeader, "echoed-in-early-hints")
		w.WriteHeader(http.StatusEarlyHints)
		w.Header().Del("Link")
		w.Header().Del(keyHeader)
	case "/trailer":
		w.Header().Set("Trailer", keyHeader)
		io.WriteString(w, "body before trailers")
		w.Header().Set(keyHeader, "echoed-in-a-trailer")
		w.Header().Set(http.TrailerPrefix+"Nocturne_Helper_Key", "echoed-in-a-late-trailer")
		return
	}
	w.Header().Set(keyHeader, "echoed-by-a-careless-listener")
	fmt.Fprintf(w, "hello from nocturne at %s for %s", r.URL.Path, r.Host)
}

func (o *origin) upgrade(w http.ResponseWriter, r *http.Request) {
	if !strings.EqualFold(r.Header.Get("Upgrade"), "websocket") {
		http.Error(w, "no upgrade", http.StatusBadRequest)
		return
	}
	conn, rw, err := http.NewResponseController(w).Hijack()
	if err != nil {
		return
	}
	defer conn.Close()
	sum := sha1.Sum([]byte(r.Header.Get("Sec-WebSocket-Key") + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))
	fmt.Fprintf(rw, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"+
		"Sec-WebSocket-Accept: %s\r\n%s: echoed-on-upgrade\r\n\r\n",
		base64.StdEncoding.EncodeToString(sum[:]), keyHeader)
	rw.Flush()
	buf := make([]byte, 64)
	n, err := rw.Read(buf)
	if err != nil {
		return
	}
	rw.Write(append([]byte("echo:"), buf[:n]...))
	rw.Flush()
}

func (o *origin) requests() []*http.Request {
	o.mu.Lock()
	defer o.mu.Unlock()
	return append([]*http.Request(nil), o.seen...)
}

func (o *origin) keys() []string {
	o.mu.Lock()
	defer o.mu.Unlock()
	return append([]string(nil), o.key...)
}

func serveOrigin(t *testing.T, ln net.Listener) *origin {
	t.Helper()
	o := &origin{}
	srv := &http.Server{Handler: o}
	go srv.Serve(ln)
	t.Cleanup(func() { srv.Close() })
	return o
}

func unixOrigin(t *testing.T) (*origin, target) {
	t.Helper()
	dir, err := os.MkdirTemp("", "tn")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	path := filepath.Join(dir, "listener.sock")
	ln, err := net.Listen("unix", path)
	if err != nil {
		t.Fatal(err)
	}
	return serveOrigin(t, ln), target{network: "unix", address: path}
}

func helperConfig(t *testing.T, controlURL string, tg target) config {
	return config{
		dir:        filepath.Join(t.TempDir(), "state"),
		hostname:   helperName,
		target:     tg,
		controlURL: controlURL,
	}
}

// running starts a helper to the running state with its certificate in place.
func running(t *testing.T, ctx context.Context, controlURL string, tg target, firstLine string) (*helperRun, *testCA) {
	t.Helper()
	ca := newTestCA(t)
	cfg := helperConfig(t, controlURL, tg)
	ca.placeCert(t, cfg.dir, helperHost)
	h := startHelper(t, ctx, cfg, firstLine)
	if ev := h.waitFor(t, stateRunning); ev.Host != helperHost {
		t.Fatalf("running on %q, want %q", ev.Host, helperHost)
	}
	return h, ca
}

const helperHost = helperName + "." + magicDomain

func stopHelper(t *testing.T, h *helperRun) {
	t.Helper()
	h.send(t, `{"cmd":"stop"}`)
	h.waitFor(t, stateStopped)
	if code := h.exitCode(t); code != 0 {
		t.Fatalf("exit code %d", code)
	}
}

// refusedTCP dials ports on addr from the phone and fails for any that connects.
func refusedTCP(t *testing.T, ctx context.Context, phone *tsnet.Server, addr netip.Addr, ports ...uint16) {
	t.Helper()
	for _, port := range ports {
		dctx, cancel := context.WithTimeout(ctx, refuseWithin)
		c, err := phone.Dial(dctx, "tcp", netip.AddrPortFrom(addr, port).String())
		cancel()
		if err == nil {
			io.WriteString(c, probeMark)
			c.Close()
			t.Errorf("the phone reached %v", netip.AddrPortFrom(addr, port))
		}
	}
}

// silentUDP sends datagrams to addr:port from the phone and fails on any reply.
func silentUDP(t *testing.T, ctx context.Context, phone *tsnet.Server, addr netip.Addr, port uint16, payload []byte) {
	t.Helper()
	c, err := phone.Dial(ctx, "udp", netip.AddrPortFrom(addr, port).String())
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	for range 3 {
		c.Write(payload)
	}
	c.SetReadDeadline(time.Now().Add(2 * time.Second))
	buf := make([]byte, 512)
	if n, err := c.Read(buf); err == nil {
		t.Errorf("%v answered over UDP: %q", netip.AddrPortFrom(addr, port), buf[:n])
	}
}

// dnsQuery is a query for helperHost's A record.
func dnsQuery() []byte {
	q := []byte{0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0}
	for _, label := range strings.Split(helperHost, ".") {
		q = append(q, byte(len(label)))
		q = append(q, label...)
	}
	return append(q, 0, 0, 1, 0, 1)
}

// probeMark starts everything the phone sends towards the computer's loopback services, so a
// listener there can tell the phone's traffic from other programs on a shared machine.
const probeMark = "nocturne-tailnet-probe"

// loopbackListeners listens on 127.0.0.1 over TCP and UDP and reports anything from the phone
// that arrives.
type loopbackListeners struct {
	tcpPort, udpPort uint16
	arrived          chan string
}

func listenOnLoopback(t *testing.T) *loopbackListeners {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	pc, err := net.ListenPacket("udp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close(); pc.Close() })
	l := &loopbackListeners{
		tcpPort: uint16(ln.Addr().(*net.TCPAddr).Port),
		udpPort: uint16(pc.LocalAddr().(*net.UDPAddr).Port),
		arrived: make(chan string, 16),
	}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go func() {
				defer c.Close()
				c.SetReadDeadline(time.Now().Add(refuseWithin))
				buf := make([]byte, len(probeMark))
				if _, err := io.ReadFull(c, buf); err == nil && string(buf) == probeMark {
					l.arrived <- "a tcp connection"
				}
			}()
		}
	}()
	go func() {
		buf := make([]byte, 64)
		for {
			n, _, err := pc.ReadFrom(buf)
			if err != nil {
				return
			}
			if strings.HasPrefix(string(buf[:n]), probeMark) {
				l.arrived <- fmt.Sprintf("a udp datagram %q", buf[:n])
			}
		}
	}()
	return l
}

func (l *loopbackListeners) expectNothing(t *testing.T) {
	t.Helper()
	select {
	case what := <-l.arrived:
		t.Fatalf("%s reached a service on the computer's 127.0.0.1", what)
	case <-time.After(2 * time.Second):
	}
}

// TestPhoneReachesOnlyTheListener is the key test: a tailnet peer reaches port 443 and its
// requests, WebSockets included, arrive at Nocturne's listener; nothing else on the computer
// is reachable through the helper, over IPv4 or IPv6, even a service on 127.0.0.1.
func TestPhoneReachesOnlyTheListener(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()
	_, controlURL := startControl(t, controlOpts{https: true})
	org, tg := unixOrigin(t)
	h, ca := running(t, ctx, controlURL, tg, "")
	phone := startPhone(t, ctx, controlURL, "phone")
	v4, v6 := helperAddrs(t, ctx, phone)

	for _, ip := range []netip.Addr{v4, v6} {
		t.Run("https reaches the listener over "+ip.String(), func(t *testing.T) {
			client := phoneClient(phone, ip, ca.pool)
			res := doWithRetry(t, client, func() *http.Request { return get(t, "https://"+helperHost+"/s/hello") })
			defer res.Body.Close()
			body, _ := io.ReadAll(res.Body)
			want := "hello from nocturne at /s/hello for " + helperHost
			if res.StatusCode != 200 || string(body) != want {
				t.Fatalf("got %d %q", res.StatusCode, body)
			}
			if got := res.Header.Values(keyHeader); len(got) != 0 {
				t.Fatalf("the key header came back: %q", got)
			}
		})
	}
	client := phoneClient(phone, v4, ca.pool)

	t.Run("websocket upgrade works", func(t *testing.T) {
		ws, br := openWebSocket(t, ctx, phone, v4, ca.pool)
		ws.SetDeadline(time.Now().Add(30 * time.Second))
		if _, err := ws.Write([]byte("ping")); err != nil {
			t.Fatal(err)
		}
		got := make([]byte, len("echo:ping"))
		if _, err := io.ReadFull(br, got); err != nil || string(got) != "echo:ping" {
			t.Fatalf("echo %q, %v", got, err)
		}
	})

	t.Run("only a websocket upgrade", func(t *testing.T) {
		before := len(org.requests())
		r := get(t, "https://"+helperHost+"/ws")
		r.Header.Set("Connection", "Upgrade")
		r.Header.Set("Upgrade", "h2c")
		if code, _, _ := status(t, client, r); code != http.StatusBadRequest {
			t.Fatalf("an h2c upgrade got %d", code)
		}
		if len(org.requests()) != before {
			t.Fatal("the refused upgrade reached the listener")
		}
	})

	t.Run("only this node's name", func(t *testing.T) {
		for _, host := range []string{"other." + magicDomain, v4.String(), helperHost + ":8443"} {
			r := get(t, "https://"+helperHost+"/s/hello")
			r.Host = host
			if code, _, _ := status(t, client, r); code != http.StatusMisdirectedRequest {
				t.Errorf("Host %q got %d", host, code)
			}
		}
		r := get(t, "https://"+helperHost+"/s/hello")
		r.Host = strings.ToUpper(helperHost) + ":443"
		if code, _, body := status(t, client, r); code != 200 || !strings.HasSuffix(body, " for "+helperHost) {
			t.Fatalf("the node's own name got %d %q", code, body)
		}
	})

	loop := listenOnLoopback(t)
	t.Run("no other tcp port, not even loopback services", func(t *testing.T) {
		for _, ip := range []netip.Addr{v4, v6} {
			refusedTCP(t, ctx, phone, ip, loop.tcpPort, 22, 53, 80, 5252, 8080, 8443, 41641)
		}
		// 100.100.100.100 is the phone's own service address; nothing sent there may reach
		// this computer's loopback either.
		quad100 := netip.AddrPortFrom(netip.MustParseAddr("100.100.100.100"), loop.tcpPort)
		dctx, dcancel := context.WithTimeout(ctx, refuseWithin)
		if c, err := phone.Dial(dctx, "tcp", quad100.String()); err == nil {
			io.WriteString(c, probeMark)
			c.Close()
		}
		dcancel()
		loop.expectNothing(t)
	})

	t.Run("no udp port, not even loopback services", func(t *testing.T) {
		for _, ip := range []netip.Addr{v4, v6} {
			silentUDP(t, ctx, phone, ip, loop.udpPort, []byte(probeMark))
			silentUDP(t, ctx, phone, ip, 53, dnsQuery())
			silentUDP(t, ctx, phone, ip, 443, []byte("quic?"))
		}
		loop.expectNothing(t)
	})

	stopHelper(t, h)
}

// TestLoopbackTargetGetsTheKey covers Windows: the listener is on 127.0.0.1 and needs
// Nocturne's key, which the helper adds; a phone can neither forge nor read it.
func TestLoopbackTargetGetsTheKey(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()
	_, controlURL := startControl(t, controlOpts{https: true})
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	org := serveOrigin(t, ln)
	tg, err := parseTarget("http://" + ln.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	h, ca := running(t, ctx, controlURL, tg, `{"key":"`+testKey+`"}`)
	phone := startPhone(t, ctx, controlURL, "phone")
	v4, _ := helperAddrs(t, ctx, phone)
	client := phoneClient(phone, v4, ca.pool)
	url := "https://" + helperHost + "/s/x"

	res := doWithRetry(t, client, func() *http.Request { return get(t, url) })
	io.Copy(io.Discard, res.Body)
	res.Body.Close()
	if res.StatusCode != 200 || len(res.Header.Values(keyHeader)) != 0 {
		t.Fatalf("status %d, key back %q", res.StatusCode, res.Header.Values(keyHeader))
	}

	t.Run("a forged key is refused in any spelling", func(t *testing.T) {
		before := len(org.requests())
		for _, name := range []string{keyHeader, "nocturne-helper-key", "Nocturne_Helper_Key"} {
			r := get(t, url)
			r.Header[name] = []string{"forged-by-the-phone-000"}
			if code, _, _ := status(t, client, r); code != http.StatusBadRequest {
				t.Errorf("%s got %d", name, code)
			}
		}
		if len(org.requests()) != before {
			t.Fatal("a request with a forged key reached the listener")
		}
	})

	t.Run("connection header cannot drop the key", func(t *testing.T) {
		r := get(t, url)
		r.Header.Set("Connection", keyHeader)
		if code, _, _ := status(t, client, r); code != 200 {
			t.Fatalf("status %d", code)
		}
	})

	t.Run("request trailers are refused", func(t *testing.T) {
		before := len(org.requests())
		r, _ := http.NewRequest("POST", url, io.MultiReader(strings.NewReader("body")))
		r.ContentLength = -1
		r.Trailer = http.Header{"X-Late": nil}
		if code, _, _ := status(t, client, r); code != http.StatusBadRequest {
			t.Fatalf("status %d", code)
		}
		if len(org.requests()) != before {
			t.Fatal("a request with trailers reached the listener")
		}
	})

	t.Run("the key never comes back in trailers", func(t *testing.T) {
		res, err := client.Do(get(t, "https://"+helperHost+"/trailer"))
		if err != nil {
			t.Fatal(err)
		}
		body, _ := io.ReadAll(res.Body)
		res.Body.Close()
		if string(body) != "body before trailers" {
			t.Fatalf("body %q", body)
		}
		for name, v := range res.Trailer {
			if isKeyName(name) && len(v) > 0 {
				t.Fatalf("trailer %s: %q", name, v)
			}
		}
	})

	t.Run("the key never comes back in early hints", func(t *testing.T) {
		var early []string
		trace := &httptrace.ClientTrace{Got1xxResponse: func(code int, h textproto.MIMEHeader) error {
			early = append(early, h.Values(keyHeader)...)
			if h.Get("Link") == "" {
				t.Errorf("the early hints did not arrive whole: %v", h)
			}
			return nil
		}}
		r := get(t, "https://"+helperHost+"/early")
		r = r.WithContext(httptrace.WithClientTrace(r.Context(), trace))
		if code, hdr, _ := status(t, client, r); code != 200 || len(hdr.Values(keyHeader)) != 0 {
			t.Fatalf("status %d, header %v", code, hdr)
		}
		if len(early) != 0 {
			t.Fatalf("the key came back in early hints: %q", early)
		}
	})

	for _, k := range org.keys() {
		if k != testKey {
			t.Fatalf("the listener saw key %q, want only the helper's", k)
		}
	}
	if got, reqs := len(org.keys()), len(org.requests()); got != reqs {
		t.Fatalf("keys %d for %d requests", got, reqs)
	}

	h.stdin.Close()
	h.waitFor(t, stateStopped)
	if code := h.exitCode(t); code != 0 {
		t.Fatalf("exit code %d after the end of input", code)
	}
	if out := h.output(); strings.Contains(out, testKey) {
		t.Fatalf("the key appeared in the output:\n%s", out)
	}
}

// openWebSocket upgrades a new connection from phone to the helper at ip and returns it.
func openWebSocket(t *testing.T, ctx context.Context, phone *tsnet.Server, ip netip.Addr, pool *x509.CertPool) (net.Conn, *bufio.Reader) {
	t.Helper()
	conn, err := phone.Dial(ctx, "tcp", netip.AddrPortFrom(ip, 443).String())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { conn.Close() })
	conn.SetDeadline(time.Now().Add(30 * time.Second))
	tc := tls.Client(conn, &tls.Config{RootCAs: pool, ServerName: helperHost})
	fmt.Fprintf(tc, "GET /ws HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"+
		"Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n", helperHost)
	br := bufio.NewReader(tc)
	res, err := http.ReadResponse(br, nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.StatusCode != http.StatusSwitchingProtocols ||
		res.Header.Get("Sec-WebSocket-Accept") != "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" {
		t.Fatalf("no upgrade: %d %v", res.StatusCode, res.Header)
	}
	if got := res.Header.Values(keyHeader); len(got) != 0 {
		t.Fatalf("the key header came back on the upgrade: %q", got)
	}
	conn.SetDeadline(time.Time{})
	return tc, br
}

// refusedAtAccept: the phone's TCP connection to :443 opens (the path works) and the helper
// closes it before TLS, so no request is ever made.
func refusedAtAccept(t *testing.T, ctx context.Context, phone *tsnet.Server, ip netip.Addr, pool *x509.CertPool) error {
	t.Helper()
	dctx, cancel := context.WithTimeout(ctx, refuseWithin)
	defer cancel()
	conn, err := phone.Dial(dctx, "tcp", netip.AddrPortFrom(ip, 443).String())
	if err != nil {
		return fmt.Errorf("no path yet: %w", err)
	}
	defer conn.Close()
	conn.SetDeadline(time.Now().Add(refuseWithin))
	tc := tls.Client(conn, &tls.Config{RootCAs: pool, ServerName: helperHost})
	if err := tc.Handshake(); err == nil {
		return errors.New("the TLS handshake succeeded")
	} else if ne, ok := err.(net.Error); ok && ne.Timeout() {
		return errors.New("the connection was held open, not closed")
	}
	return nil
}

// TestOnlyOwnDevicesGetIn: another person's device, a tagged device and a device shared in
// from elsewhere are refused before TLS, and a device that loses access mid-session has its
// open connections, WebSockets included, closed.
func TestOnlyOwnDevicesGetIn(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()

	t.Run("another user's device", func(t *testing.T) {
		_, controlURL := startControl(t, controlOpts{https: true, otherUsers: true})
		org, tg := unixOrigin(t)
		h, ca := running(t, ctx, controlURL, tg, "")
		stranger := startPhone(t, ctx, controlURL, "stranger")
		v4, _ := helperAddrs(t, ctx, stranger)
		if err := tstest.WaitFor(stepTimeout, func() error {
			return refusedAtAccept(t, ctx, stranger, v4, ca.pool)
		}); err != nil {
			t.Fatal(err)
		}
		if n := len(org.requests()); n != 0 {
			t.Fatalf("%d requests reached the listener", n)
		}
		stopHelper(t, h)
	})

	control, controlURL := startControl(t, controlOpts{https: true})
	org, tg := unixOrigin(t)
	h, ca := running(t, ctx, controlURL, tg, "")
	phone := startPhone(t, ctx, controlURL, "phone")
	v4, _ := helperAddrs(t, ctx, phone)
	client := phoneClient(phone, v4, ca.pool)
	res := doWithRetry(t, client, func() *http.Request { return get(t, "https://"+helperHost+"/s/hello") })
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("the person's own phone got %d", res.StatusCode)
	}
	lc, err := phone.LocalClient()
	if err != nil {
		t.Fatal(err)
	}
	st, err := lc.Status(ctx)
	if err != nil {
		t.Fatal(err)
	}
	own := control.Node(st.Self.PublicKey)

	for _, c := range []struct {
		name   string
		change func(n *tailcfg.Node)
	}{
		{"a shared device", func(n *tailcfg.Node) { n.Sharer = n.User + 1 }},
		{"a tagged device", func(n *tailcfg.Node) { n.Tags = []string{"tag:phone"} }},
		{"a device moved to another owner", func(n *tailcfg.Node) { n.User++ }},
	} {
		t.Run(c.name, func(t *testing.T) {
			ws, br := openWebSocket(t, ctx, phone, v4, ca.pool)
			n := own.Clone()
			c.change(n)
			control.UpdateNode(n)

			ws.SetReadDeadline(time.Now().Add(stepTimeout))
			if _, err := br.ReadByte(); err == nil {
				t.Fatal("the open WebSocket carried data instead of closing")
			} else if ne, ok := err.(net.Error); ok && ne.Timeout() {
				t.Fatal("the open WebSocket stayed open after the device lost access")
			}
			before := len(org.requests())
			if err := tstest.WaitFor(stepTimeout, func() error {
				return refusedAtAccept(t, ctx, phone, v4, ca.pool)
			}); err != nil {
				t.Fatal(err)
			}
			if len(org.requests()) != before {
				t.Fatal("a refused device's request reached the listener")
			}

			control.UpdateNode(own.Clone())
			if err := tstest.WaitFor(stepTimeout, func() error {
				res, err := client.Do(get(t, "https://"+helperHost+"/s/hello"))
				if err != nil {
					return err
				}
				res.Body.Close()
				if res.StatusCode != 200 {
					return fmt.Errorf("status %d", res.StatusCode)
				}
				return nil
			}); err != nil {
				t.Fatalf("after undoing the change: %v", err)
			}
		})
	}
	t.Run("this computer moved to another account", func(t *testing.T) {
		var helperNode *tailcfg.Node
		for _, n := range control.AllNodes() {
			if n.Hostinfo.Hostname() == helperName {
				helperNode = control.Node(n.Key)
			}
		}
		if helperNode == nil {
			t.Fatal("the control server does not know the helper")
		}
		ws, br := openWebSocket(t, ctx, phone, v4, ca.pool)
		helperNode.User = 999
		control.UpdateNode(helperNode)
		ws.SetReadDeadline(time.Now().Add(stepTimeout))
		if _, err := br.ReadByte(); err == nil {
			t.Fatal("the open WebSocket carried data instead of closing")
		} else if ne, ok := err.(net.Error); ok && ne.Timeout() {
			t.Fatal("the old account's WebSocket stayed open")
		}
		if err := tstest.WaitFor(stepTimeout, func() error {
			return refusedAtAccept(t, ctx, phone, v4, ca.pool)
		}); err != nil {
			t.Fatalf("the old account's phone still gets in: %v", err)
		}
	})
	stopHelper(t, h)
}

// TestSignInThenSignOut walks the person's first run: a sign-in link, then running, then
// signing out from Nocturne.
func TestSignInThenSignOut(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()
	control, controlURL := startControl(t, controlOpts{https: true, requireAuth: true})
	ca := newTestCA(t)
	_, tg := unixOrigin(t)
	cfg := helperConfig(t, controlURL, tg)
	ca.placeCert(t, cfg.dir, helperHost)

	h := startHelper(t, ctx, cfg, "")
	ev := h.waitFor(t, stateNeedsLogin)
	if !strings.HasPrefix(ev.AuthURL, controlURL+"/auth/") {
		t.Fatalf("sign-in link %q", ev.AuthURL)
	}
	h.send(t, `{"cmd":"status"}`)
	if again := h.waitFor(t, stateNeedsLogin); again != ev {
		t.Fatalf("status said %+v, want %+v", again, ev)
	}
	if !control.CompleteAuth(ev.AuthURL) {
		t.Fatal("the control server did not know the sign-in link")
	}
	h.waitFor(t, stateRunning)
	h.send(t, `{"cmd":"logout"}`)
	h.waitFor(t, stateStopped)
	if code := h.exitCode(t); code != 0 {
		t.Fatalf("exit code %d", code)
	}
}

func TestValidAuthURL(t *testing.T) {
	for _, c := range []struct {
		url, control string
		ok           bool
	}{
		{"https://login.tailscale.com/a/abc123", "", true},
		{"https://controlplane.tailscale.com/a/abc123", "", true},
		{"http://login.tailscale.com/a/abc123", "", false},
		{"https://login.tailscale.com.evil.example/a/1", "", false},
		{"https://user@login.tailscale.com/a/1", "", false},
		{"javascript:alert(1)", "", false},
		{"https://evil.example/a/1", "", false},
		{"http://127.0.0.1:4000/auth/x", "http://127.0.0.1:4000", true},
		{"http://127.0.0.1:4001/auth/x", "http://127.0.0.1:4000", false},
		{"https://login.tailscale.com/a/" + strings.Repeat("a", 2048), "", false},
	} {
		if got := validAuthURL(c.url, c.control); got != c.ok {
			t.Errorf("%q with control %q: %v", c.url, c.control, got)
		}
	}
}

// TestNeedsHTTPS: a tailnet without HTTPS certificates gets no listener, and the helper says
// what the person must turn on.
func TestNeedsHTTPS(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()
	_, controlURL := startControl(t, controlOpts{})
	_, tg := unixOrigin(t)
	h := startHelper(t, ctx, helperConfig(t, controlURL, tg), "")
	h.waitFor(t, stateNeedsHTTPS)
	h.send(t, `{"cmd":"status"}`)
	h.waitFor(t, stateNeedsHTTPS)
	stopHelper(t, h)
}

// TestTailnetThatRequiresLogsIsBlocked: a tailnet with audit logging stops nodes that do not
// upload logs; the helper says so rather than waiting at starting.
func TestTailnetThatRequiresLogsIsBlocked(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 4*stepTimeout)
	defer cancel()
	control, controlURL := startControl(t, controlOpts{https: true})
	_, tg := unixOrigin(t)
	h, _ := running(t, ctx, controlURL, tg, "")
	var nodeKey key.NodePublic
	for _, n := range control.AllNodes() {
		if n.Hostinfo.Hostname() == helperName {
			nodeKey = n.Key
		}
	}
	control.SetNodeCapMap(nodeKey, tailcfg.NodeCapMap{nodecap.DataPlaneAuditLogs: nil})
	if ev := h.waitFor(t, stateError); ev.Reason != reasonBlocked {
		t.Fatalf("got %+v", ev)
	}
	stopHelper(t, h)
}

// hangingControl accepts connections and never answers, so the backend never runs.
func hangingControl(t *testing.T) string {
	t.Helper()
	release := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-release:
		case <-r.Context().Done():
		}
	}))
	t.Cleanup(srv.Close)
	t.Cleanup(func() { close(release) })
	return srv.URL
}

// TestStopsWhileTheBackendNeverRuns: stop and the end of input work while the control server
// hangs and the backend never reaches running.
func TestStopsWhileTheBackendNeverRuns(t *testing.T) {
	for _, how := range []string{"stop", "end of input"} {
		t.Run(how, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), stepTimeout)
			defer cancel()
			_, tg := unixOrigin(t)
			h := startHelper(t, ctx, helperConfig(t, hangingControl(t), tg), "")
			h.waitFor(t, stateStarting)
			time.Sleep(2 * time.Second)
			began := time.Now()
			if how == "stop" {
				h.send(t, `{"cmd":"stop"}`)
			} else {
				h.stdin.Close()
			}
			h.waitFor(t, stateStopped)
			if code := h.exitCode(t); code != 0 {
				t.Fatalf("exit code %d", code)
			}
			if took := time.Since(began); took > shutdownTimeout+5*time.Second {
				t.Fatalf("stopping took %v", took)
			}
		})
	}
}
