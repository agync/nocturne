package main

import (
	"context"
	"crypto/tls"
	"net"
	"sync"
	"time"

	"tailscale.com/client/tailscale/apitype"
	"tailscale.com/tailcfg"
)

const (
	// maxConns caps the admitted connections served at once.
	maxConns = 64
	// maxPending caps the connections still waiting for their peer check.
	maxPending = 8
	// recheckAfter is how long a passed peer check is trusted before the next request asks again.
	recheckAfter = 30 * time.Second
	whoIsTimeout = 10 * time.Second
)

// allowFunc reports whether the tailnet peer at remoteAddr may use the helper while self is
// this node's user.
type allowFunc func(ctx context.Context, remoteAddr string, self tailcfg.UserID) bool

// whoIsFunc reports who is on the other end of a tailnet connection.
type whoIsFunc func(ctx context.Context, remoteAddr string) (*apitype.WhoIsResponse, error)

// ownDevicesOnly admits the person's own devices: nodes of self's user, not tagged and not
// shared in from elsewhere.
func ownDevicesOnly(whoIs whoIsFunc) allowFunc {
	return func(ctx context.Context, remoteAddr string, self tailcfg.UserID) bool {
		if self.IsZero() {
			return false
		}
		ctx, cancel := context.WithTimeout(ctx, whoIsTimeout)
		defer cancel()
		who, err := whoIs(ctx, remoteAddr)
		return err == nil && ownDevice(who, self)
	}
}

func ownDevice(who *apitype.WhoIsResponse, self tailcfg.UserID) bool {
	if who == nil || who.Node == nil || who.UserProfile == nil || self.IsZero() {
		return false
	}
	n := who.Node
	return !n.IsTagged() && n.Sharer.IsZero() && n.User == self && who.UserProfile.ID == self
}

// peerListener admits a tailnet connection only after its peer passes the check, before any
// TLS or HTTP. A refused connection is closed at once and never holds a slot; at most
// maxPending wait for the check, and more are closed unchecked.
//
// Each admission is bound to the node's user at the time it was checked. A change of user
// moves the epoch and closes every connection; a check that began before the change is
// refused when it finishes.
type peerListener struct {
	inner   net.Listener
	allow   allowFunc
	pending chan struct{}
	slots   chan struct{}
	conns   chan *admittedConn
	done    chan struct{}

	closeOnce sync.Once
	mu        sync.Mutex
	// afterAllow, if set, runs between a connection's check and its admission. Tests only.
	afterAllow func()
	self       tailcfg.UserID
	epoch      uint64
	open       map[*admittedConn]struct{}
}

func newPeerListener(inner net.Listener, allow allowFunc, self tailcfg.UserID, pending, conns int) *peerListener {
	l := &peerListener{
		inner:   inner,
		allow:   allow,
		pending: make(chan struct{}, pending),
		slots:   make(chan struct{}, conns),
		conns:   make(chan *admittedConn),
		done:    make(chan struct{}),
		self:    self,
		open:    map[*admittedConn]struct{}{},
	}
	go l.acceptLoop()
	return l
}

// identity is the node's user and the epoch it belongs to.
func (l *peerListener) identity() (tailcfg.UserID, uint64) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.self, l.epoch
}

// setSelf records the node's user. A different user moves the epoch and closes every admitted
// connection; it reports whether that happened. Zero allows no one.
func (l *peerListener) setSelf(self tailcfg.UserID) bool {
	l.mu.Lock()
	if self == l.self {
		l.mu.Unlock()
		return false
	}
	l.self = self
	l.epoch++
	l.mu.Unlock()
	l.closeAll()
	return true
}

func (l *peerListener) acceptLoop() {
	defer l.Close()
	for {
		c, err := l.inner.Accept()
		if err != nil {
			return
		}
		select {
		case l.pending <- struct{}{}:
			go l.admit(c)
		default:
			c.Close()
		}
	}
}

func (l *peerListener) admit(c net.Conn) {
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		select {
		case <-l.done:
			cancel()
		case <-ctx.Done():
		}
	}()
	self, epoch := l.identity()
	ok := l.allow(ctx, c.RemoteAddr().String(), self)
	cancel()
	<-l.pending
	l.mu.Lock()
	hook := l.afterAllow
	l.mu.Unlock()
	if hook != nil {
		hook()
	}
	if !ok {
		c.Close()
		return
	}
	select {
	case l.slots <- struct{}{}:
	default:
		c.Close()
		return
	}
	ac := &admittedConn{Conn: c, l: l, user: self, checkedAt: time.Now(), allowed: true}
	l.mu.Lock()
	if l.epoch != epoch {
		l.mu.Unlock()
		c.Close()
		<-l.slots
		return
	}
	l.open[ac] = struct{}{}
	l.mu.Unlock()
	select {
	case l.conns <- ac:
	case <-l.done:
		ac.Close()
	}
}

func (l *peerListener) Accept() (net.Conn, error) {
	select {
	case c := <-l.conns:
		return c, nil
	case <-l.done:
		return nil, net.ErrClosed
	}
}

func (l *peerListener) Addr() net.Addr { return l.inner.Addr() }

func (l *peerListener) Close() error {
	l.closeOnce.Do(func() { close(l.done) })
	return l.inner.Close()
}

func (l *peerListener) snapshot() []*admittedConn {
	l.mu.Lock()
	defer l.mu.Unlock()
	open := make([]*admittedConn, 0, len(l.open))
	for c := range l.open {
		open = append(open, c)
	}
	return open
}

// revalidate checks every open connection's peer again and closes those no longer allowed,
// WebSockets included. The helper calls it when the tailnet's peers change.
func (l *peerListener) revalidate(ctx context.Context) {
	for _, c := range l.snapshot() {
		c.recheck(ctx, 0)
	}
}

// closeAll closes every admitted connection, WebSockets included.
func (l *peerListener) closeAll() {
	for _, c := range l.snapshot() {
		c.mu.Lock()
		c.allowed = false
		c.mu.Unlock()
		c.Close()
	}
}

// admittedConn is a connection whose peer passed the check, for which user, and when.
type admittedConn struct {
	net.Conn
	l    *peerListener
	user tailcfg.UserID

	mu        sync.Mutex
	checkedAt time.Time
	allowed   bool
	closeOnce sync.Once
}

// recheck refuses at once when the node's user is no longer the one the connection was
// admitted for, whatever the age of the last check; asks again when that check is older than
// maxAge (zero: always); and closes the connection when the peer is no longer allowed.
func (c *admittedConn) recheck(ctx context.Context, maxAge time.Duration) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	if self, _ := c.l.identity(); self != c.user {
		c.allowed = false
	}
	if c.allowed && (maxAge == 0 || time.Since(c.checkedAt) >= maxAge) {
		c.allowed = c.l.allow(ctx, c.RemoteAddr().String(), c.user)
		c.checkedAt = time.Now()
	}
	if !c.allowed {
		go c.Close()
	}
	return c.allowed
}

func (c *admittedConn) Close() error {
	err := c.Conn.Close()
	c.closeOnce.Do(func() {
		c.l.mu.Lock()
		delete(c.l.open, c)
		c.l.mu.Unlock()
		<-c.l.slots
	})
	return err
}

// admittedFrom finds the admitted connection under an HTTP server's connection.
func admittedFrom(c net.Conn) *admittedConn {
	if tc, ok := c.(*tls.Conn); ok {
		c = tc.NetConn()
	}
	ac, _ := c.(*admittedConn)
	return ac
}
