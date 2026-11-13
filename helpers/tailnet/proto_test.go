package main

import (
	"bytes"
	"strings"
	"testing"
)

func TestParseKey(t *testing.T) {
	if k, err := parseKey(`{"key":"` + testKey + `"}`); err != nil || k != testKey {
		t.Fatalf("got %q, %v", k, err)
	}
	if k, err := parseKey(` {"key":"` + testKey + `"} `); err != nil || k != testKey {
		t.Fatalf("surrounding spaces: %q, %v", k, err)
	}
	for _, in := range []string{"", `{"key":""}`, `{"key":"` + strings.Repeat("a", 257) + `"}`,
		`{"key":"0123456789abcdef\r\nX: y"}`, `{"Key":"0123456789abcdef","cmd":"stop"}`, `not json`,
		`{"key":"` + testKey + `"}{"key":"` + testKey + `"}`, `{"key":"` + testKey + `"} trailing`} {
		if _, err := parseKey(in); err == nil {
			t.Errorf("%q accepted", in)
		}
	}
}

func TestParseCommand(t *testing.T) {
	cases := map[string]string{
		`{"cmd":"status"}`:          cmdStatus,
		`{"cmd":"logout"}`:          cmdLogout,
		`{"cmd":"stop"}`:            cmdStop,
		` {"cmd":"stop"} `:          cmdStop,
		`{"cmd":"serve"}`:           "",
		`{"cmd":"funnel"}`:          "",
		`{"cmd":"stop","port":22}`:  "",
		`{"cmd":"STOP"}`:            "",
		`{"cmd":"stop"}{"cmd":"x"}`: "",
		`{"cmd":"stop"} junk`:       "",
		`{"key":"` + testKey + `"}`: "",
		`stop`:                      "",
		``:                          "",
	}
	for in, want := range cases {
		if got := parseCommand(in); got != want {
			t.Errorf("%q: got %q, want %q", in, got, want)
		}
	}
}

func TestLinesAreJSONAndStatusRepeatsTheState(t *testing.T) {
	var out bytes.Buffer
	em := newEmitter(&out)
	em.emit(event{State: stateStarting})
	em.emitIfChanged(event{State: stateNeedsLogin, AuthURL: "https://login.example/a/1?x=1&y=2"})
	em.emitIfChanged(event{State: stateNeedsLogin, AuthURL: "https://login.example/a/1?x=1&y=2"})
	em.emit(event{State: stateError, Reason: reasonCertificate})
	em.repeat()
	em.emitIfChanged(event{State: stateRunning, Host: "nocturne-ab12.example.ts.net"})
	em.emit(event{State: stateStopped})
	want := strings.Join([]string{
		`{"state":"starting"}`,
		`{"state":"needsLogin","authUrl":"https://login.example/a/1?x=1&y=2"}`,
		`{"state":"error","reason":"certificate"}`,
		`{"state":"needsLogin","authUrl":"https://login.example/a/1?x=1&y=2"}`,
		`{"state":"running","host":"nocturne-ab12.example.ts.net"}`,
		`{"state":"stopped"}`,
	}, "\n") + "\n"
	if out.String() != want {
		t.Fatalf("got\n%s\nwant\n%s", out.String(), want)
	}
}

func TestReadLines(t *testing.T) {
	read := func(in string) []inputLine {
		out := make(chan inputLine, 8)
		readLines(strings.NewReader(in), out, nil)
		close(out)
		var got []inputLine
		for l := range out {
			got = append(got, l)
		}
		return got
	}
	got := read(`{"cmd":"status"}` + "\n" + `{"cmd":"stop"}`)
	if len(got) != 3 || got[0].text != `{"cmd":"status"}` || got[1].text != `{"cmd":"stop"}` ||
		!got[2].end || got[2].err != nil {
		t.Fatalf("got %+v", got)
	}
	got = read(strings.Repeat("x", maxLine+1) + "\n" + `{"cmd":"stop"}` + "\n")
	if len(got) != 1 || !got[0].end || got[0].err != errLineTooLong {
		t.Fatalf("an overlong line: %+v", got)
	}
}
