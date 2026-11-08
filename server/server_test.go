package main

import (
	"encoding/json"
	"net"
	"testing"
	"time"
)

type tc struct {
	t    *testing.T
	conn net.Conn
}

func dial(t *testing.T, addr string) *tc {
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { conn.Close() })
	return &tc{t, conn}
}

func (c *tc) send(v any) {
	c.conn.Write(jsonFrame(v))
}

// next reads frames until pred matches a JSON message.
func (c *tc) until(pred func(m map[string]any) bool) map[string]any {
	c.conn.SetReadDeadline(time.Now().Add(5 * time.Second))
	for {
		f, err := readFrame(c.conn)
		if err != nil {
			c.t.Fatalf("read: %v", err)
		}
		if f.kind != kindJSON {
			continue
		}
		var m map[string]any
		json.Unmarshal(f.payload, &m)
		if pred(m) {
			return m
		}
	}
}

func typ(s string) func(map[string]any) bool {
	return func(m map[string]any) bool { return m["t"] == s }
}

func testHub(t *testing.T) (*hub, string) {
	h := newHub(config{MaxSpeed: 64, MaxRooms: 5, MaxPerIP: 20, MaxConns: 50, RoomsPerKey: 2, DataDir: t.TempDir()})
	h.load()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	go h.serve(l)
	t.Cleanup(func() { l.Close() })
	return h, l.Addr().String()
}

func hello(c *tc, name string) {
	c.send(map[string]any{"t": "hello", "proto": protoVersion, "name": name, "key": name + "-key"})
	c.until(typ("welcome"))
}

func TestTurnsRelayedInOrderToAllPlayers(t *testing.T) {
	_, addr := testHub(t)
	a, b := dial(t, addr), dial(t, addr)
	hello(a, "alice")
	hello(b, "bob")
	a.send(map[string]any{"t": "create", "name": "Test", "seed": 7})
	j := a.until(typ("joined"))
	room := j["room"].(map[string]any)["id"].(string)
	b.send(map[string]any{"t": "join", "room": room})
	b.until(typ("joined"))
	a.send(map[string]any{"t": "cmd", "c": map[string]any{"Remove": map[string]any{"x": 1}}})
	isCmdTurn := func(m map[string]any) bool {
		return m["t"] == "turn" && len(m["cmds"].([]any)) > 0
	}
	ta, tb := a.until(isCmdTurn), b.until(isCmdTurn)
	if ta["n"] != tb["n"] || ta["ticks"] != tb["ticks"] {
		t.Fatalf("players saw different turns: %v vs %v", ta, tb)
	}
	if ta["ticks"].(float64) != 2 {
		t.Fatalf("1x should be 2 ticks per turn, got %v", ta["ticks"])
	}
}

func TestLateJoinerGetsSnapshotAndTail(t *testing.T) {
	h, addr := testHub(t)
	h.cfg.SnapEvery = 30
	a := dial(t, addr)
	hello(a, "alice")
	a.send(map[string]any{"t": "create", "name": "Snap", "seed": 1})
	j := a.until(typ("joined"))
	id := j["room"].(map[string]any)["id"].(string)
	var req map[string]any
	// Keep acking so the room never throttles, until the snapshot is requested.
	a.conn.SetReadDeadline(time.Now().Add(10 * time.Second))
	for req == nil {
		f, err := readFrame(a.conn)
		if err != nil {
			t.Fatalf("waiting for snapreq: %v", err)
		}
		var m map[string]any
		json.Unmarshal(f.payload, &m)
		switch m["t"] {
		case "turn":
			a.send(map[string]any{"t": "ack", "n": m["n"]})
		case "snapreq":
			req = m
		}
	}
	n := uint64(req["n"].(float64))
	a.conn.Write(snapshotFrame(n, 0, []byte("STATE")))
	time.Sleep(200 * time.Millisecond)
	r := h.rooms[id]
	r.mu.Lock()
	gotTurn, logLen, snap := r.snapTurn, len(r.log), string(r.snap)
	r.mu.Unlock()
	if gotTurn != n || snap != "STATE" {
		t.Fatalf("snapshot not stored: turn=%d want %d snap=%q", gotTurn, n, snap)
	}
	if logLen > 30 {
		t.Fatalf("log not trimmed: %d turns", logLen)
	}
	b := dial(t, addr)
	hello(b, "bob")
	b.send(map[string]any{"t": "join", "room": id})
	jb := b.until(typ("joined"))
	if jb["has_snap"] != true || uint64(jb["snap_turn"].(float64)) != n {
		t.Fatalf("joiner header wrong: %v", jb)
	}
}

func TestHashMismatchTriggersResync(t *testing.T) {
	_, addr := testHub(t)
	a, b := dial(t, addr), dial(t, addr)
	hello(a, "alice")
	hello(b, "bob")
	a.send(map[string]any{"t": "create", "name": "Hash", "seed": 1})
	id := a.until(typ("joined"))["room"].(map[string]any)["id"].(string)
	b.send(map[string]any{"t": "join", "room": id})
	b.until(typ("joined"))
	turn := a.until(typ("turn"))
	n := turn["n"]
	a.send(map[string]any{"t": "hash", "n": n, "hash": 111})
	b.send(map[string]any{"t": "hash", "n": n, "hash": 222})
	// Tie goes to the lowest id (alice, who joined first), so bob must resync.
	b.until(typ("resync"))
}

func TestRoomRulesAndOwnerDelete(t *testing.T) {
	h, addr := testHub(t)
	a := dial(t, addr)
	hello(a, "alice")
	a.send(map[string]any{"t": "create", "name": "Locked", "pass": "pw", "max": 1})
	id := a.until(typ("joined"))["room"].(map[string]any)["id"].(string)
	b := dial(t, addr)
	hello(b, "bob")
	b.send(map[string]any{"t": "join", "room": id, "pass": "nope"})
	if m := b.until(typ("error")); m["msg"] != "Wrong room password" {
		t.Fatal(m)
	}
	b.send(map[string]any{"t": "join", "room": id, "pass": "pw"})
	if m := b.until(typ("error")); m["msg"] != "Room is full" {
		t.Fatal(m)
	}
	b.send(map[string]any{"t": "delete", "room": id})
	b.until(typ("error"))
	a.send(map[string]any{"t": "leave"})
	a.until(typ("left"))
	a.send(map[string]any{"t": "delete", "room": id})
	a.until(typ("rooms"))
	h.mu.Lock()
	n := len(h.rooms)
	h.mu.Unlock()
	if n != 0 {
		t.Fatalf("room not deleted: %d", n)
	}
}

func TestPersistenceRoundTrip(t *testing.T) {
	h, addr := testHub(t)
	a := dial(t, addr)
	hello(a, "alice")
	a.send(map[string]any{"t": "create", "name": "Keep", "seed": 99})
	id := a.until(typ("joined"))["room"].(map[string]any)["id"].(string)
	a.send(map[string]any{"t": "cmd", "c": map[string]any{"x": 1}})
	a.until(func(m map[string]any) bool { return m["t"] == "turn" && len(m["cmds"].([]any)) > 0 })
	h.saveDirty()
	h2 := newHub(h.cfg)
	h2.load()
	r := h2.rooms[id]
	if r == nil || r.seed != 99 || len(r.log) == 0 {
		t.Fatalf("room not restored: %+v", r)
	}
}

func TestHostedWorldBecomesRoomBaseSnapshot(t *testing.T) {
	h, addr := testHub(t)
	a := dial(t, addr)
	hello(a, "alice")
	a.send(map[string]any{"t": "create", "name": "Mine", "seed": 5, "base": true})
	id := a.until(typ("joined"))["room"].(map[string]any)["id"].(string)
	// Not listed or joinable until the world arrives.
	b := dial(t, addr)
	hello(b, "bob")
	b.send(map[string]any{"t": "join", "room": id})
	if m := b.until(typ("error")); m["msg"] != "Room is still being set up" {
		t.Fatal(m)
	}
	a.conn.Write(snapshotFrame(0, 4242, []byte("WORLD")))
	deadline := time.Now().Add(3 * time.Second)
	for {
		h.mu.Lock()
		r := h.rooms[id]
		h.mu.Unlock()
		r.mu.Lock()
		ready, snap, tick := !r.pendingBase, string(r.snap), r.tick
		r.mu.Unlock()
		if ready {
			if snap != "WORLD" || tick != 4242 {
				t.Fatalf("base wrong: %q %d", snap, tick)
			}
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("base never accepted")
		}
		time.Sleep(20 * time.Millisecond)
	}
	b.send(map[string]any{"t": "join", "room": id})
	jb := b.until(typ("joined"))
	if jb["has_snap"] != true || jb["snap_turn"].(float64) != 0 {
		t.Fatalf("joiner should start from the hosted world: %v", jb)
	}
}

func TestAbandonedHostedRoomIsRemoved(t *testing.T) {
	h, addr := testHub(t)
	a := dial(t, addr)
	hello(a, "alice")
	a.send(map[string]any{"t": "create", "name": "Gone", "base": true})
	a.until(typ("joined"))
	a.conn.Close()
	deadline := time.Now().Add(3 * time.Second)
	for {
		h.mu.Lock()
		n := len(h.rooms)
		h.mu.Unlock()
		if n == 0 {
			return
		}
		if time.Now().After(deadline) {
			t.Fatal("abandoned room still present")
		}
		time.Sleep(20 * time.Millisecond)
	}
}
