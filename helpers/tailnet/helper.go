package main

import (
	"context"
	"crypto/tls"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"time"

	"tailscale.com/client/local"
	"tailscale.com/ipn"
	"tailscale.com/tailcfg"
	"tailscale.com/tsnet"
	"tailscale.com/types/logger"
)

type config struct {
	dir      string
	hostname string
	target   target
	// controlURL is empty in the shipped helper (Tailscale's own); tests point it at an
	// in-process control server.
	controlURL string
}

const (
	recheckEvery    = 5 * time.Second
	certRetryEvery  = 30 * time.Second
	certTimeout     = 2 * time.Minute
	logoutTimeout   = 30 * time.Second
	shutdownTimeout = 10 * time.Second
	localTimeout    = 10 * time.Second
	selfRetryBase   = time.Second
	maxSelfRetry    = 30 * time.Second
)

func discardLogger() *log.Logger { return log.New(io.Discard, "", 0) }

func warn(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "tailnet: "+format+"\n", a...)
}

// run serves until stop, logout, the end of input or ctx ends. It returns the exit code.
func run(ctx context.Context, cfg config, in io.Reader, out io.Writer) int {
	em := newEmitter(out)
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	lines := make(chan inputLine)
	go readLines(in, lines, ctx.Done())

	var key string
	if cfg.target.needsKey() {
		select {
		case <-ctx.Done():
			em.emit(event{State: stateStopped})
			return 0
		case l := <-lines:
			var err error
			if l.end {
				err = errBadKey
				if l.err != nil {
					err = l.err
				}
			} else {
				key, err = parseKey(l.text)
			}
			if err != nil {
				reason := reasonKey
				if errors.Is(err, errLineTooLong) {
					reason = reasonInput
				}
				em.emit(event{State: stateError, Reason: reason})
				return 1
			}
		}
	}

	em.emit(event{State: stateStarting})
	h := &helper{
		cfg: cfg,
		key: key,
		em:  em,
		srv: &tsnet.Server{
			Dir:        cfg.dir,
			Hostname:   cfg.hostname,
			ControlURL: cfg.controlURL,
			UserLogf:   logger.Discard,
		},
		startDone:    make(chan struct{}),
		peersChanged: make(chan struct{}, 1),
		setupCh:      make(chan setupResult, 1),
		certCh:       make(chan bool, 1),
	}
	code := h.loop(ctx, lines)
	cancel()
	h.shutdown()
	em.emit(event{State: stateStopped})
	return code
}

type helper struct {
	cfg config
	key string
	em  *emitter
	srv *tsnet.Server
	lc  *local.Client
	web *http.Server
	// peers admits tailnet connections; nil until the listener is up.
	peers *peerListener
	// peersChanged wakes the worker that checks open connections again.
	peersChanged chan struct{}

	startDone chan struct{}
	setupCh   chan setupResult
	certCh    chan bool

	state       ipn.State
	authURL     string
	wasRunning  bool
	askedLogin  bool
	blocked     bool
	settingUp   bool
	listening   bool
	serveFailed bool
	host        string
	certReady   bool
}

// started is what the backend start hands the loop: notifications, or why it failed.
type started struct {
	lc       *local.Client
	notes    <-chan *ipn.Notify
	watchErr <-chan error
	err      error
}

// start brings the backend up and watches it. It runs beside the loop so that stop, the end
// of input and signals are answered even while the backend is slow.
func (h *helper) start(ctx context.Context) started {
	defer close(h.startDone)
	if err := h.srv.Start(); err != nil {
		return started{err: err}
	}
	lc, err := h.srv.LocalClient()
	if err != nil {
		return started{err: err}
	}
	watcher, err := lc.WatchIPNBus(ctx, ipn.NotifyInitialState|ipn.NotifyPeerChanges)
	if err != nil {
		return started{err: err}
	}
	notes := make(chan *ipn.Notify)
	watchErr := make(chan error, 1)
	go func() {
		defer watcher.Close()
		for {
			n, err := watcher.Next()
			if err != nil {
				watchErr <- err
				return
			}
			select {
			case notes <- &n:
			case <-ctx.Done():
				return
			}
		}
	}()
	return started{lc: lc, notes: notes, watchErr: watchErr}
}

func (h *helper) loop(ctx context.Context, lines <-chan inputLine) int {
	startCh := make(chan started, 1)
	go func() { startCh <- h.start(ctx) }()
	var notes <-chan *ipn.Notify
	var watchErr <-chan error
	var recheck <-chan time.Time
	for {
		select {
		case <-ctx.Done():
			return 0
		case l := <-lines:
			if l.end {
				if l.err != nil {
					warn("reading commands failed: %v", l.err)
					h.em.emit(event{State: stateError, Reason: reasonInput})
					return 1
				}
				return 0
			}
			switch parseCommand(l.text) {
			case cmdStatus:
				h.em.repeat()
			case cmdStop:
				return 0
			case cmdLogout:
				h.logout(ctx)
				return 0
			default:
				warn("ignored a line that is not a command")
			}
		case st := <-startCh:
			if st.err != nil {
				if ctx.Err() != nil {
					return 0
				}
				warn("start failed: %v", st.err)
				h.em.emit(event{State: stateError, Reason: reasonStart})
				return 1
			}
			h.lc, notes, watchErr = st.lc, st.notes, st.watchErr
		case err := <-watchErr:
			if ctx.Err() != nil {
				return 0
			}
			warn("watch ended: %v", err)
			h.em.emit(event{State: stateError, Reason: reasonBackend})
			return 1
		case n := <-notes:
			h.onNotify(ctx, n)
		case res := <-h.setupCh:
			h.onSetup(ctx, res)
		case ok := <-h.certCh:
			h.certReady = ok
			if ok {
				h.showRunning()
			} else {
				h.em.emit(event{State: stateError, Reason: reasonCertificate})
			}
		case <-recheck:
			h.onState(ctx)
		}
		recheck = nil
		if h.waiting() {
			recheck = time.After(recheckEvery)
		}
	}
}

// waiting reports whether the person has a step to take that no notification announces:
// turning on HTTPS certificates, or a sign-in link that has not arrived yet.
func (h *helper) waiting() bool {
	switch h.state {
	case ipn.Running:
		return !h.listening && !h.serveFailed && !h.settingUp
	case ipn.NeedsLogin:
		return h.authURL == ""
	}
	return false
}

func (h *helper) onNotify(ctx context.Context, n *ipn.Notify) {
	if n.ErrMessage != nil {
		warn("backend reported an error")
		if n.Prefs != nil && n.Prefs.Valid() && !n.Prefs.WantRunning() {
			h.reportBlocked()
		} else {
			h.em.emit(event{State: stateError, Reason: reasonBackend})
		}
	}
	if n.State != nil || n.SelfChange != nil || len(n.PeersChanged) > 0 || len(n.PeersRemoved) > 0 {
		h.revalidatePeers()
	}
	if n.BrowseToURL != nil && *n.BrowseToURL != "" {
		h.authURL = *n.BrowseToURL
	}
	if n.State != nil {
		if *n.State != h.state && *n.State == ipn.NeedsLogin {
			h.authURL = ""
			if n.BrowseToURL != nil {
				h.authURL = *n.BrowseToURL
			}
		}
		h.state = *n.State
	}
	if n.State != nil || n.BrowseToURL != nil {
		h.onState(ctx)
	}
}

// revalidatePeers wakes the worker that checks every open connection again after the
// tailnet's peers changed; it closes those whose device was removed, tagged, shared in or moved
// to another owner.
func (h *helper) revalidatePeers() {
	select {
	case h.peersChanged <- struct{}{}:
	default:
	}
}

// selfUserFunc reads this node's user from the backend.
type selfUserFunc func(ctx context.Context) (tailcfg.UserID, error)

func (h *helper) selfUserFromStatus(ctx context.Context) (tailcfg.UserID, error) {
	ctx, cancel := context.WithTimeout(ctx, localTimeout)
	defer cancel()
	st, err := h.lc.StatusWithoutPeers(ctx)
	if err != nil {
		return 0, err
	}
	if st.Self == nil {
		return 0, nil
	}
	return st.Self.UserID, nil
}

// watchPeers keeps admitted connections in line with the tailnet: on each wake-up it reads
// this node's user again (a different one, as after signing into another account, closes
// every connection) and checks every open connection again. When the user cannot be read it
// fails closed, allowing no one, and retries with backoff.
func watchPeers(ctx context.Context, peers *peerListener, selfUser selfUserFunc, wake <-chan struct{}, retryBase time.Duration) {
	var retry <-chan time.Time
	backoff := retryBase
	for {
		select {
		case <-wake:
		case <-retry:
		case <-ctx.Done():
			return
		}
		retry = nil
		id, err := selfUser(ctx)
		if err != nil {
			if ctx.Err() != nil {
				return
			}
			warn("reading this computer's account failed; refusing every phone until it reads")
			peers.setSelf(0)
			retry = time.After(backoff)
			backoff = min(2*backoff, maxSelfRetry)
			continue
		}
		backoff = retryBase
		if peers.setSelf(id) {
			warn("this computer's account changed; closed every connection")
		}
		peers.revalidate(ctx)
	}
}

// reportBlocked says once that the tailnet stopped this node: it requires log upload, which
// this helper never does.
func (h *helper) reportBlocked() {
	if !h.blocked {
		h.blocked = true
		h.em.emit(event{State: stateError, Reason: reasonBlocked})
	}
}

func (h *helper) onState(ctx context.Context) {
	switch h.state {
	case ipn.NeedsLogin:
		if h.authURL == "" {
			lctx, cancel := context.WithTimeout(ctx, localTimeout)
			if st, err := h.lc.StatusWithoutPeers(lctx); err == nil {
				h.authURL = st.AuthURL
			}
			cancel()
		}
		if h.authURL != "" {
			if !validAuthURL(h.authURL, h.cfg.controlURL) {
				warn("refused a sign-in link that is not on the control server")
				h.em.emit(event{State: stateError, Reason: reasonLogin})
				return
			}
			h.em.emitIfChanged(event{State: stateNeedsLogin, AuthURL: h.authURL})
			return
		}
		if h.wasRunning && !h.askedLogin {
			h.askedLogin = true
			go func() {
				lctx, cancel := context.WithTimeout(ctx, localTimeout)
				defer cancel()
				if err := h.lc.StartLoginInteractive(lctx); err != nil {
					warn("asking for a sign-in link failed: %v", err)
				}
			}()
		}
	case ipn.NeedsMachineAuth:
		h.em.emitIfChanged(event{State: stateNeedsApproval})
	case ipn.Stopped:
		h.reportBlocked()
	case ipn.Running:
		h.wasRunning = true
		h.askedLogin = false
		h.blocked = false
		h.authURL = ""
		h.serve(ctx)
	}
}

// setupResult is the outcome of preparing the listener.
type setupResult struct {
	needsHTTPS bool
	ln         net.Listener
	host       string
	self       tailcfg.UserID
	err        error
}

func (h *helper) serve(ctx context.Context) {
	if h.listening {
		h.showRunning()
		return
	}
	if h.settingUp || h.serveFailed {
		return
	}
	h.settingUp = true
	go func() {
		res := h.setup(ctx)
		select {
		case h.setupCh <- res:
		case <-ctx.Done():
			if res.ln != nil {
				res.ln.Close()
			}
		}
	}()
}

// setup checks that the tailnet gives this node a certificate name, clears any serve
// configuration (so no Funnel) and opens the one tailnet listener on port 443.
func (h *helper) setup(ctx context.Context) setupResult {
	sctx, cancel := context.WithTimeout(ctx, localTimeout)
	defer cancel()
	st, err := h.lc.StatusWithoutPeers(sctx)
	if err != nil {
		return setupResult{err: err}
	}
	if st.CurrentTailnet == nil || !st.CurrentTailnet.MagicDNSEnabled || len(st.CertDomains) == 0 {
		return setupResult{needsHTTPS: true}
	}
	if st.Self == nil {
		return setupResult{err: errors.New("no self in status")}
	}
	if err := h.lc.SetServeConfig(sctx, new(ipn.ServeConfig)); err != nil {
		return setupResult{err: err}
	}
	ln, err := h.srv.Listen("tcp", ":443")
	if err != nil {
		return setupResult{err: err}
	}
	return setupResult{ln: ln, host: st.CertDomains[0], self: st.Self.UserID}
}

func (h *helper) onSetup(ctx context.Context, res setupResult) {
	h.settingUp = false
	switch {
	case res.needsHTTPS:
		h.em.emitIfChanged(event{State: stateNeedsHTTPS})
		return
	case res.err != nil:
		if ctx.Err() != nil {
			return
		}
		warn("listen failed: %v", res.err)
		h.serveFailed = true
		h.em.emit(event{State: stateError, Reason: reasonListen})
		return
	}
	h.listening = true
	h.host = res.host
	h.peers = newPeerListener(res.ln, ownDevicesOnly(h.lc.WhoIs), res.self, maxPending, maxConns)
	go watchPeers(ctx, h.peers, h.selfUserFromStatus, h.peersChanged, selfRetryBase)
	h.web = newHTTPServer(&gate{
		host:    res.host,
		recheck: recheckAfter,
		next:    newProxy(h.cfg.target, h.key, res.host),
	})
	tlsLn := tls.NewListener(h.peers, &tls.Config{
		GetCertificate: h.lc.GetCertificate,
		MinVersion:     tls.VersionTLS12,
	})
	web := h.web
	go func() {
		if err := web.Serve(tlsLn); err != nil && !errors.Is(err, http.ErrServerClosed) &&
			!errors.Is(err, net.ErrClosed) {
			warn("serving ended: %v", err)
		}
	}()
	go h.warmCertificate(ctx, h.host)
}

// warmCertificate fetches the certificate before saying running, so a phone's first visit
// does not wait for it.
func (h *helper) warmCertificate(ctx context.Context, host string) {
	for {
		cctx, cancel := context.WithTimeout(ctx, certTimeout)
		_, _, err := h.lc.CertPair(cctx, host)
		cancel()
		if ctx.Err() != nil {
			return
		}
		select {
		case h.certCh <- err == nil:
		case <-ctx.Done():
			return
		}
		if err == nil {
			return
		}
		warn("certificate failed: %v", err)
		select {
		case <-time.After(certRetryEvery):
		case <-ctx.Done():
			return
		}
	}
}

func (h *helper) showRunning() {
	if h.certReady && h.state == ipn.Running {
		h.em.emitIfChanged(event{State: stateRunning, Host: h.host})
	}
}

func (h *helper) logout(ctx context.Context) {
	if h.lc == nil {
		h.em.emit(event{State: stateError, Reason: reasonLogout})
		return
	}
	lctx, cancel := context.WithTimeout(ctx, logoutTimeout)
	defer cancel()
	if err := h.lc.Logout(lctx); err != nil {
		warn("sign out failed: %v", err)
		h.em.emit(event{State: stateError, Reason: reasonLogout})
	}
}

// shutdown closes the listener and the node. Close must not run while Start does, so it
// waits for the start to finish, within the same deadline.
func (h *helper) shutdown() {
	deadline := time.After(shutdownTimeout)
	if h.web != nil {
		_ = h.web.Close()
	}
	select {
	case <-h.startDone:
	case <-deadline:
		warn("shutdown timed out while the backend was starting")
		return
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		_ = h.srv.Close()
	}()
	select {
	case <-done:
	case <-deadline:
		warn("shutdown timed out")
	}
}

// validAuthURL accepts a sign-in link only on the control server: Tailscale's login hosts
// over https, or the configured control server's own scheme and host.
func validAuthURL(raw, controlURL string) bool {
	if len(raw) > 2048 {
		return false
	}
	u, err := url.Parse(raw)
	if err != nil || u.User != nil || u.Host == "" {
		return false
	}
	if controlURL == "" {
		return u.Scheme == "https" && (u.Host == "login.tailscale.com" || u.Host == "controlplane.tailscale.com")
	}
	c, err := url.Parse(controlURL)
	return err == nil && u.Scheme == c.Scheme && u.Host == c.Host
}
