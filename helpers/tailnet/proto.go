package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"sync"
)

const (
	stateStarting      = "starting"
	stateNeedsLogin    = "needsLogin"
	stateNeedsApproval = "needsApproval"
	stateNeedsHTTPS    = "needsHttps"
	stateRunning       = "running"
	stateError         = "error"
	stateStopped       = "stopped"
)

const (
	cmdStatus = "status"
	cmdLogout = "logout"
	cmdStop   = "stop"
)

const (
	reasonKey         = "key"
	reasonStart       = "start"
	reasonListen      = "listen"
	reasonCertificate = "certificate"
	reasonLogout      = "logout"
	reasonBackend     = "backend"
	reasonInput       = "input"
	reasonBlocked     = "blocked"
	reasonLogin       = "login"
)

const maxLine = 4096

type event struct {
	State   string `json:"state"`
	AuthURL string `json:"authUrl,omitempty"`
	Host    string `json:"host,omitempty"`
	Reason  string `json:"reason,omitempty"`
}

type command struct {
	Cmd string `json:"cmd"`
}

type keyLine struct {
	Key string `json:"key"`
}

// emitter writes one JSON object per line and remembers the last state for status.
type emitter struct {
	mu   sync.Mutex
	w    io.Writer
	last event
}

func newEmitter(w io.Writer) *emitter {
	return &emitter{w: w}
}

func (e *emitter) emit(ev event) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if ev.State != stateError {
		e.last = ev
	}
	e.writeLocked(ev)
}

// emitIfChanged skips an event equal to the last state, so a repeated notification
// does not repeat a line.
func (e *emitter) emitIfChanged(ev event) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if ev == e.last {
		return
	}
	e.last = ev
	e.writeLocked(ev)
}

func (e *emitter) repeat() {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.writeLocked(e.last)
}

func (e *emitter) writeLocked(ev event) {
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(ev); err != nil {
		return
	}
	_, _ = e.w.Write(b.Bytes())
}

var errBadKey = errors.New("the first line must carry the key")

// decodeLine decodes exactly one JSON object with only known fields, and nothing after it.
func decodeLine(line string, v any) bool {
	dec := json.NewDecoder(strings.NewReader(line))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		return false
	}
	_, err := dec.Token()
	return err == io.EOF
}

// parseKey checks the first line, which must be {"key":"..."} with a key fit for a header value.
func parseKey(line string) (string, error) {
	var k keyLine
	if !decodeLine(line, &k) || !validKey(k.Key) {
		return "", errBadKey
	}
	return k.Key, nil
}

func validKey(k string) bool {
	if len(k) < 16 || len(k) > 256 {
		return false
	}
	for i := 0; i < len(k); i++ {
		c := k[i]
		ok := c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' ||
			c == '-' || c == '_' || c == '.' || c == '~'
		if !ok {
			return false
		}
	}
	return true
}

// parseCommand returns the command name, or "" for a line that is not a known command.
func parseCommand(line string) string {
	var c command
	if !decodeLine(line, &c) {
		return ""
	}
	switch c.Cmd {
	case cmdStatus, cmdLogout, cmdStop:
		return c.Cmd
	}
	return ""
}

// inputLine is one line of stdin, or its end: err is nil at a clean end of input and set for
// a read error or a line over maxLine.
type inputLine struct {
	text string
	end  bool
	err  error
}

var errLineTooLong = errors.New("an input line is too long")

// readLines sends each line of r, then one final inputLine with end set. It is the only reader
// of stdin, so waiting for a line never stops the helper from noticing a signal.
func readLines(r io.Reader, out chan<- inputLine, done <-chan struct{}) {
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 0, maxLine), maxLine)
	send := func(l inputLine) bool {
		select {
		case out <- l:
			return true
		case <-done:
			return false
		}
	}
	for sc.Scan() {
		if !send(inputLine{text: sc.Text()}) {
			return
		}
	}
	err := sc.Err()
	if errors.Is(err, bufio.ErrTooLong) {
		err = errLineTooLong
	}
	send(inputLine{end: true, err: err})
}
