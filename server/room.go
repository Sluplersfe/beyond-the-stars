package main

import (
	"encoding/json"
	"sort"
	"sync"
	"time"
)

const (
	protoVersion = 1

	turnInterval  = 100 * time.Millisecond
	ticksPerSec   = 20 // must match sim::TICKS_PER_SEC
	hashEvery     = 50
	throttleLag   = 50
	syncedLag     = 20
	lagGrace      = 10 * time.Second
	maxPending    = 4000
	logChunkBytes = 60 << 10
)

type turnRec struct {
	Frame []byte
	Tick  uint64 // sim tick after this turn
}

type pendingCmd struct {
	p uint32
	c json.RawMessage
}

type room struct {
	hub *hub

	mu      sync.Mutex
	id      string
	name    string
	pass    string
	owner   string // owner key hash
	seed    uint64
	max     int
	simVer  string
	created time.Time
	active  time.Time

	speed  int
	paused bool

	// pendingBase: a hosted world whose creator has not uploaded its starting snapshot yet.
	pendingBase bool
	creator     uint32

	turn    uint64
	tick    uint64
	players map[uint32]*client
	pending []pendingCmd

	snap     []byte
	snapTurn uint64
	snapTick uint64
	snapAsk  uint64 // turn most recently requested (0: none outstanding)
	snapAt   time.Time
	log      []turnRec

	hashes map[uint64]map[uint32]uint64
	dirty  bool
	quit   chan struct{}
}

func (r *room) info() roomInfo {
	return roomInfo{
		ID: r.id, Name: r.name, Players: len(r.players), Max: r.max, Seed: r.seed,
		Tick: r.tick, Locked: r.pass != "", Speed: r.speed, Paused: r.paused,
	}
}

func (r *room) playerList() []playerInfo {
	out := make([]playerInfo, 0, len(r.players))
	for _, c := range r.players {
		out = append(out, playerInfo{c.id, c.name})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

// broadcast sends a prebuilt frame to everyone in the room. Caller holds r.mu.
func (r *room) broadcast(b []byte) {
	for _, c := range r.players {
		c.send(b)
	}
}

func (r *room) broadcastPlayers() {
	r.broadcast(jsonFrame(map[string]any{"t": "players", "players": r.playerList()}))
}

func (r *room) system(text string) {
	r.broadcast(jsonFrame(map[string]any{"t": "chat", "p": 0, "name": "", "text": text}))
}

// uniqueName makes a name unused within the room. Caller holds r.mu.
func (r *room) uniqueName(name string) string {
	taken := func(n string) bool {
		for _, c := range r.players {
			if c.name == n {
				return true
			}
		}
		return false
	}
	if !taken(name) {
		return name
	}
	for i := 2; ; i++ {
		n := name + " " + itoa(i)
		if !taken(n) {
			return n
		}
	}
}

func itoa(i int) string {
	b, _ := json.Marshal(i)
	return string(b)
}

// add puts a client in the room and sends it everything needed to catch up. Caller holds r.mu.
func (r *room) add(c *client) {
	c.name = r.uniqueName(c.name)
	r.players[c.id] = c
	c.room = r
	r.active = time.Now()
	r.sendState(c, true)
	r.broadcastPlayers()
	r.system(c.name + " joined")
}

// sendState sends the join payload: header, snapshot, then every turn since the snapshot.
func (r *room) sendState(c *client, first bool) {
	c.synced = false
	c.ack = r.snapTurn
	c.lagSince = time.Time{}
	c.send(jsonFrame(map[string]any{
		"t": "joined", "room": r.info(), "you": c.id, "name": c.name, "seed": r.seed,
		"snap_turn": r.snapTurn, "has_snap": r.snap != nil, "head": r.turn,
		"players": r.playerList(), "max_speed": r.hub.maxSpeed, "first": first,
	}))
	if r.snap != nil {
		c.send(snapshotFrame(r.snapTurn, r.snapTick, r.snap))
	}
	var chunk []byte
	for _, t := range r.log {
		chunk = append(chunk, t.Frame...)
		if len(chunk) >= logChunkBytes {
			c.send(chunk)
			chunk = nil
		}
	}
	if len(chunk) > 0 {
		c.send(chunk)
	}
}

// remove drops a client. Caller holds r.mu.
func (r *room) remove(c *client) {
	if r.players[c.id] != c {
		return
	}
	delete(r.players, c.id)
	c.room = nil
	r.active = time.Now()
	r.snapAsk = 0 // the uploader may have been the one leaving
	if len(r.players) > 0 {
		r.broadcastPlayers()
		r.system(c.name + " left")
	}
}

func (r *room) uploader() *client {
	var best *client
	for _, c := range r.players {
		if c.synced && (best == nil || c.id < best.id) {
			best = c
		}
	}
	return best
}

// throttled reports whether a synced player is too far behind for the room to run ahead.
func (r *room) throttled(now time.Time) bool {
	for _, c := range r.players {
		if !c.synced {
			continue
		}
		if r.turn > c.ack && r.turn-c.ack > throttleLag {
			if c.lagSince.IsZero() {
				c.lagSince = now
			}
			if now.Sub(c.lagSince) > lagGrace {
				c.synced = false // stop waiting for a stalled client
				continue
			}
			return true
		}
		c.lagSince = time.Time{}
	}
	return false
}

// step produces the next turn if there is anything to do.
func (r *room) step(now time.Time) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if len(r.players) == 0 || r.pendingBase {
		return
	}
	ticks := 0
	if !r.paused {
		ticks = r.speed * ticksPerSec * int(turnInterval/time.Millisecond) / 1000
	}
	if ticks > 0 && r.throttled(now) {
		return
	}
	if ticks == 0 && len(r.pending) == 0 {
		return
	}
	r.turn++
	r.tick += uint64(ticks)
	cmds := make([]map[string]any, len(r.pending))
	for i, p := range r.pending {
		cmds[i] = map[string]any{"p": p.p, "c": p.c}
	}
	r.pending = r.pending[:0]
	frame := jsonFrame(map[string]any{"t": "turn", "n": r.turn, "ticks": ticks, "cmds": cmds})
	r.log = append(r.log, turnRec{frame, r.tick})
	r.broadcast(frame)
	r.dirty = true
	r.active = now
	if r.snapAsk != 0 && now.Sub(r.snapAt) > 30*time.Second {
		r.snapAsk = 0 // the uploader never answered
	}
	if r.turn-r.snapTurn >= r.hub.cfg.SnapEvery && r.snapAsk == 0 {
		if u := r.uploader(); u != nil {
			r.snapAsk = r.turn
			r.snapAt = now
			u.send(jsonFrame(map[string]any{"t": "snapreq", "n": r.turn}))
		}
	}
}

func (r *room) run() {
	t := time.NewTicker(turnInterval)
	defer t.Stop()
	for {
		select {
		case now := <-t.C:
			r.step(now)
		case <-r.quit:
			return
		}
	}
}

// acceptSnapshot stores an uploaded snapshot and trims the log it covers.
func (r *room) acceptSnapshot(c *client, turn, tick uint64, data []byte) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.pendingBase {
		// The creator's own world is the room's starting state (turn 0).
		if c.id == r.creator && turn == 0 && r.players[c.id] == c {
			r.snap, r.snapTurn, r.snapTick, r.tick = data, 0, tick, tick
			r.pendingBase = false
			r.dirty = true
		}
		return
	}
	if r.players[c.id] != c || r.snapAsk == 0 || turn != r.snapAsk {
		return
	}
	if turn <= r.snapTurn || turn > r.turn {
		return
	}
	idx := int(turn - r.snapTurn)
	if idx > len(r.log) {
		return
	}
	r.snapTick = r.log[idx-1].Tick
	r.snap = data
	r.snapTurn = turn
	r.log = append([]turnRec(nil), r.log[idx:]...)
	r.snapAsk = 0
	r.dirty = true
}

// reportHash records a client's state hash and tells outliers to resync.
func (r *room) reportHash(c *client, turn, hash uint64) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.players[c.id] != c || turn == 0 || turn > r.turn {
		return
	}
	if r.hashes == nil {
		r.hashes = map[uint64]map[uint32]uint64{}
	}
	m := r.hashes[turn]
	if m == nil {
		m = map[uint32]uint64{}
		r.hashes[turn] = m
	}
	m[c.id] = hash
	for old := range r.hashes {
		if old+200 < turn {
			delete(r.hashes, old)
		}
	}
	if len(m) < 2 {
		return
	}
	votes := map[uint64]int{}
	for _, h := range m {
		votes[h]++
	}
	var truth uint64
	best, lowest := 0, ^uint32(0)
	for id, h := range m {
		if v := votes[h]; v > best || (v == best && id < lowest) {
			best, lowest, truth = v, id, h
		}
	}
	if len(votes) == 1 {
		return
	}
	for id, h := range m {
		if h != truth {
			if p := r.players[id]; p != nil {
				p.send(jsonFrame(map[string]any{"t": "resync", "n": turn}))
			}
		}
	}
	delete(r.hashes, turn)
}
