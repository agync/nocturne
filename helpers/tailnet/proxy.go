package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httputil"
	"net/netip"
	"net/url"
	"path/filepath"
	"strings"
	"time"
)

// keyHeader carries Nocturne's helper key to a loopback TCP target (Windows). Unix socket
// targets need no key: the socket's permissions are the key.
const keyHeader = "Nocturne-Helper-Key"

type target struct {
	network string // "unix" or "tcp"
	address string // socket path, or 127.0.0.1:<port>
}

func (t target) needsKey() bool { return t.network == "tcp" }

var errBadTarget = errors.New("target must be unix:<absolute path> or http://127.0.0.1:<port>")

func parseTarget(s string) (target, error) {
	if p, ok := strings.CutPrefix(s, "unix:"); ok {
		if p == "" || !filepath.IsAbs(p) {
			return target{}, errBadTarget
		}
		return target{network: "unix", address: filepath.Clean(p)}, nil
	}
	u, err := url.Parse(s)
	if err != nil || u.Scheme != "http" || u.User != nil || u.RawQuery != "" || u.Fragment != "" ||
		(u.Path != "" && u.Path != "/") {
		return target{}, errBadTarget
	}
	ap, err := netip.ParseAddrPort(u.Host)
	if err != nil || ap.Addr() != netip.MustParseAddr("127.0.0.1") || ap.Port() == 0 {
		return target{}, errBadTarget
	}
	return target{network: "tcp", address: ap.String()}, nil
}

// gate admits a request only on a connection whose peer still passes the check (asked again
// once the last answer is older than recheck), for this node's name, without the key header
// in any spelling, without trailers, and upgrading to nothing but WebSocket.
type gate struct {
	host    string
	recheck time.Duration
	next    http.Handler
}

type admittedKey struct{}

func (g *gate) connContext(ctx context.Context, c net.Conn) context.Context {
	return context.WithValue(ctx, admittedKey{}, admittedFrom(c))
}

func (g *gate) peerAllowed(r *http.Request) bool {
	ac, _ := r.Context().Value(admittedKey{}).(*admittedConn)
	return ac != nil && ac.recheck(r.Context(), g.recheck)
}

func (g *gate) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	switch {
	case !g.peerAllowed(r):
		w.Header().Set("Connection", "close")
		w.WriteHeader(http.StatusForbidden)
	case !sameHost(r.Host, g.host):
		w.WriteHeader(http.StatusMisdirectedRequest)
	case hasKey(r.Header), len(r.Trailer) > 0, !upgradeAllowed(r.Header):
		w.WriteHeader(http.StatusBadRequest)
	default:
		g.next.ServeHTTP(&keylessWriter{ResponseWriter: w}, r)
		stripKey(w.Header())
	}
}

func sameHost(got, want string) bool {
	if h, port, err := net.SplitHostPort(got); err == nil {
		if port != "443" {
			return false
		}
		got = h
	}
	return want != "" && strings.EqualFold(strings.TrimSuffix(got, "."), want)
}

func upgradeAllowed(h http.Header) bool {
	up := h.Values("Upgrade")
	if len(up) == 0 {
		return true
	}
	return len(up) == 1 && strings.EqualFold(strings.TrimSpace(up[0]), "websocket")
}

// keylessWriter drops the key header from every header block sent through it: 1xx and final.
// The gate strips the map once more after the proxy returns, which covers trailers.
type keylessWriter struct {
	http.ResponseWriter
}

func (k *keylessWriter) WriteHeader(code int) {
	stripKey(k.Header())
	k.ResponseWriter.WriteHeader(code)
}

func (k *keylessWriter) Write(b []byte) (int, error) {
	stripKey(k.Header())
	return k.ResponseWriter.Write(b)
}

func (k *keylessWriter) Unwrap() http.ResponseWriter { return k.ResponseWriter }

// newProxy forwards HTTP and WebSocket requests to the target, as host, adding the real key
// when there is one.
func newProxy(t target, key, host string) http.Handler {
	dialer := &net.Dialer{Timeout: 10 * time.Second}
	transport := &http.Transport{
		Proxy: nil,
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			return dialer.DialContext(ctx, t.network, t.address)
		},
		MaxIdleConns:       16,
		IdleConnTimeout:    90 * time.Second,
		DisableCompression: true,
	}
	return &httputil.ReverseProxy{
		Rewrite: func(pr *httputil.ProxyRequest) {
			pr.Out.URL.Scheme = "http"
			pr.Out.URL.Host = "nocturne"
			pr.Out.Host = host
			stripKey(pr.Out.Header)
			pr.Out.Trailer = nil
			if key != "" {
				pr.Out.Header.Set(keyHeader, key)
			}
		},
		Transport:     transport,
		FlushInterval: -1,
		ModifyResponse: func(res *http.Response) error {
			stripKey(res.Header)
			stripKey(res.Trailer)
			return nil
		},
		ErrorHandler: func(w http.ResponseWriter, _ *http.Request, _ error) {
			w.WriteHeader(http.StatusBadGateway)
		},
	}
}

// isKeyName matches the key header however it is spelled: any case, underscores for
// hyphens, or as a trailer.
func isKeyName(name string) bool {
	name = strings.TrimPrefix(name, http.TrailerPrefix)
	return strings.EqualFold(strings.ReplaceAll(name, "_", "-"), keyHeader)
}

func hasKey(h http.Header) bool {
	for name := range h {
		if isKeyName(name) {
			return true
		}
	}
	return false
}

// stripKey removes the key header in any spelling, and its name from a Trailer announcement.
func stripKey(h http.Header) {
	for name := range h {
		if isKeyName(name) {
			delete(h, name)
		}
	}
	announced := h.Values("Trailer")
	if len(announced) == 0 {
		return
	}
	var keep []string
	for _, v := range announced {
		for _, name := range strings.Split(v, ",") {
			if name = strings.TrimSpace(name); name != "" && !isKeyName(name) {
				keep = append(keep, name)
			}
		}
	}
	h.Del("Trailer")
	if len(keep) > 0 {
		h.Set("Trailer", strings.Join(keep, ", "))
	}
}

func newHTTPServer(g *gate) *http.Server {
	return &http.Server{
		Handler:           g,
		ConnContext:       g.connContext,
		ReadHeaderTimeout: 10 * time.Second,
		IdleTimeout:       2 * time.Minute,
		MaxHeaderBytes:    32 << 10,
		ErrorLog:          discardLogger(),
	}
}
