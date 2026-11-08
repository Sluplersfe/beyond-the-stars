package main

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"log"
	"net"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode"
)

type config struct {
	Addr         string
	DataDir      string
	ServerPass   string
	MaxSpeed     int
	MaxRooms     int
	MaxPerIP     int
	MaxConns     int
	RoomsPerKey  int
	IdleRoomDays int
	SnapEvery    uint64 // turns between snapshot uploads
}

type hub struct {
	cfg      config
	maxSpeed int

	mu      sync.Mutex
	rooms   map[string]*room
	conns   int
	perIP   map[string]int
	nextCID uint32
}

func newHub(cfg config) *hub {
	if cfg.SnapEvery == 0 {
		cfg.SnapEvery = 300
	}
	return &hub{cfg: cfg, maxSpeed: cfg.MaxSpeed, rooms: map[string]*room{}, perIP: map[string]int{}}
}

type bucket struct {
	tokens float64
	rate   float64
	burst  float64
	last   time.Time
}

func newBucket(rate, burst float64) *bucket {
	return &bucket{tokens: burst, rate: rate, burst: burst, last: time.Now()}
}

func (b *bucket) take() bool {
	now := time.Now()
	b.tokens += now.Sub(b.last).Seconds() * b.rate
	if b.tokens > b.burst {
		b.tokens = b.burst
	}
	b.last = now
	if b.tokens < 1 {
		return false
	}
	b.tokens--
	return true
}

type client struct {
	id   uint32
	conn net.Conn
	out  chan []byte
	dead atomic.Bool
	ip   string

	// Guarded by the room mutex (or only touched by the read loop before joining).
	name     string
	room     *room
	synced   bool
	ack      uint64
	lagSince time.Time

	key   string // hashed owner key
	hello bool

	cmds, chats, misc *bucket
}

func (c *client) send(b []byte) {
	if c.dead.Load() {
		return
	}
	select {
	case c.out <- b:
	default:
		log.Printf("client %d (%s): outbound queue full, dropping", c.id, c.name)
		c.kill()
	}
}

func (c *client) kill() {
	if c.dead.CompareAndSwap(false, true) {
		c.conn.Close()
	}
}

func (c *client) writer() {
	for b := range c.out {
		c.conn.SetWriteDeadline(time.Now().Add(20 * time.Second))
		if _, err := c.conn.Write(b); err != nil {
			c.kill()
			return
		}
	}
}

func cleanText(s string, max int) string {
	var sb strings.Builder
	n := 0
	for _, r := range strings.TrimSpace(s) {
		if unicode.IsControl(r) || !unicode.IsPrint(r) {
			continue
		}
		if n >= max {
			break
		}
		sb.WriteRune(r)
		n++
	}
	return strings.TrimSpace(sb.String())
}

func hashKey(k string) string {
	if k == "" {
		return ""
	}
	h := sha256.Sum256([]byte("bts-owner:" + k))
	return hex.EncodeToString(h[:8])
}

func randID() string {
	var b [4]byte
	rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func (h *hub) serve(l net.Listener) {
	for {
		conn, err := l.Accept()
		if err != nil {
			log.Printf("accept: %v", err)
			return
		}
		go h.handle(conn)
	}
}

func (h *hub) admit(ip string) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.conns >= h.cfg.MaxConns || h.perIP[ip] >= h.cfg.MaxPerIP {
		return false
	}
	h.conns++
	h.perIP[ip]++
	return true
}

func (h *hub) release(ip string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.conns--
	if h.perIP[ip]--; h.perIP[ip] <= 0 {
		delete(h.perIP, ip)
	}
}

func (h *hub) roomList() []roomInfo {
	h.mu.Lock()
	rooms := make([]*room, 0, len(h.rooms))
	for _, r := range h.rooms {
		rooms = append(rooms, r)
	}
	h.mu.Unlock()
	out := make([]roomInfo, 0, len(rooms))
	for _, r := range rooms {
		r.mu.Lock()
		if !r.pendingBase {
			out = append(out, r.info())
		}
		r.mu.Unlock()
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Players != out[j].Players {
			return out[i].Players > out[j].Players
		}
		return out[i].Name < out[j].Name
	})
	return out
}

func (h *hub) handle(conn net.Conn) {
	ip, _, _ := net.SplitHostPort(conn.RemoteAddr().String())
	if !h.admit(ip) {
		conn.Close()
		return
	}
	defer h.release(ip)
	if tc, ok := conn.(*net.TCPConn); ok {
		tc.SetNoDelay(true)
		tc.SetKeepAlive(true)
		tc.SetKeepAlivePeriod(30 * time.Second)
	}
	c := &client{
		conn: conn, out: make(chan []byte, 1024), ip: ip,
		cmds: newBucket(60, 120), chats: newBucket(0.5, 4), misc: newBucket(20, 40),
	}
	h.mu.Lock()
	h.nextCID++
	c.id = h.nextCID
	h.mu.Unlock()
	go c.writer()
	defer func() {
		h.leave(c)
		c.kill()
		close(c.out)
		log.Printf("disconnect %d %q %s", c.id, c.name, ip)
	}()

	conn.SetReadDeadline(time.Now().Add(10 * time.Second))
	for {
		f, err := readFrame(conn)
		if err != nil {
			return
		}
		conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		if f.kind == kindSnapshot {
			h.onSnapshot(c, f.payload)
			continue
		}
		var m inbound
		if err := jsonUnmarshal(f.payload, &m); err != nil {
			return
		}
		if !c.hello && m.T != "hello" {
			return
		}
		if !h.dispatch(c, &m) {
			return
		}
	}
}

func (h *hub) fail(c *client, msg string) {
	c.send(jsonFrame(map[string]any{"t": "error", "msg": msg}))
}

func (h *hub) leave(c *client) {
	if r := c.currentRoom(); r != nil {
		r.mu.Lock()
		r.remove(c)
		abandoned := r.pendingBase // the creator left before uploading the world
		r.mu.Unlock()
		if abandoned {
			h.removeRoom(r)
		}
	}
}

// currentRoom reads c.room safely: it is only written under the room lock, but the read
// loop is the sole other accessor, so a stale read at worst re-checks membership under lock.
func (c *client) currentRoom() *room { return c.room }

func (h *hub) dispatch(c *client, m *inbound) bool {
	if !c.misc.take() {
		return true
	}
	switch m.T {
	case "hello":
		if c.hello {
			return false
		}
		if m.Proto != protoVersion {
			h.fail(c, "Protocol mismatch: update the game (server speaks v"+itoa(protoVersion)+")")
			return false
		}
		if h.cfg.ServerPass != "" && m.Pass != h.cfg.ServerPass {
			h.fail(c, "Wrong server password")
			return false
		}
		c.name = cleanText(m.Name, 20)
		if c.name == "" {
			c.name = "Player"
		}
		c.key = hashKey(m.Key)
		c.hello = true
		c.send(jsonFrame(map[string]any{
			"t": "welcome", "id": c.id, "proto": protoVersion, "max_speed": h.maxSpeed,
			"rooms": h.roomList(), "motd": "Beyond the Stars multiplayer",
		}))
		log.Printf("hello %d %q %s sim=%s", c.id, c.name, c.ip, m.Sim)
	case "rooms":
		c.send(jsonFrame(map[string]any{"t": "rooms", "rooms": h.roomList()}))
	case "create":
		h.create(c, m)
	case "join":
		h.join(c, m)
	case "leave":
		h.leave(c)
		c.send(jsonFrame(map[string]any{"t": "left"}))
	case "delete":
		h.deleteRoom(c, m)
	case "ping":
		c.send(jsonFrame(map[string]any{"t": "pong", "n": m.N}))
	case "cmd":
		r := c.room
		if r == nil || len(m.C) == 0 || len(m.C) > 4096 {
			return true
		}
		if !c.cmds.take() {
			return true
		}
		r.mu.Lock()
		if r.players[c.id] == c && len(r.pending) < maxPending {
			r.pending = append(r.pending, pendingCmd{c.id, m.C})
		}
		r.mu.Unlock()
	case "speed":
		if r := c.room; r != nil {
			r.mu.Lock()
			if r.players[c.id] == c {
				v := m.V
				if v < 1 {
					v = 1
				}
				if v > h.maxSpeed {
					v = h.maxSpeed
				}
				if v != r.speed {
					r.speed = v
					r.dirty = true
					r.broadcast(jsonFrame(map[string]any{"t": "state", "speed": r.speed, "paused": r.paused, "by": c.id}))
				}
			}
			r.mu.Unlock()
		}
	case "pause":
		if r := c.room; r != nil {
			r.mu.Lock()
			if r.players[c.id] == c && r.paused != m.On {
				r.paused = m.On
				r.dirty = true
				r.broadcast(jsonFrame(map[string]any{"t": "state", "speed": r.speed, "paused": r.paused, "by": c.id}))
			}
			r.mu.Unlock()
		}
	case "chat":
		r := c.room
		text := cleanText(m.Text, 200)
		if r == nil || text == "" || !c.chats.take() {
			return true
		}
		r.mu.Lock()
		if r.players[c.id] == c {
			r.broadcast(jsonFrame(map[string]any{"t": "chat", "p": c.id, "name": c.name, "text": text}))
		}
		r.mu.Unlock()
	case "ack":
		if r := c.room; r != nil {
			r.mu.Lock()
			if r.players[c.id] == c {
				if m.N > c.ack && m.N <= r.turn {
					c.ack = m.N
				}
				if r.turn-c.ack <= syncedLag {
					c.synced = true
				}
			}
			r.mu.Unlock()
		}
	case "hash":
		if r := c.room; r != nil {
			r.reportHash(c, m.N, m.Hash)
		}
	case "resync":
		if r := c.room; r != nil {
			r.mu.Lock()
			if r.players[c.id] == c {
				r.sendState(c, false)
			}
			r.mu.Unlock()
		}
	default:
		return false
	}
	return true
}

func (h *hub) onSnapshot(c *client, p []byte) {
	r := c.room
	if r == nil || len(p) < 16 {
		return
	}
	turn := binary.BigEndian.Uint64(p[:8])
	tick := binary.BigEndian.Uint64(p[8:16])
	r.acceptSnapshot(c, turn, tick, append([]byte(nil), p[16:]...))
}

func (h *hub) create(c *client, m *inbound) {
	if c.room != nil {
		h.fail(c, "Leave your current room first")
		return
	}
	name := cleanText(m.Name, 32)
	if name == "" {
		h.fail(c, "Room needs a name")
		return
	}
	max := m.Max
	if max < 1 || max > 16 {
		max = 8
	}
	pass := cleanText(m.Pass, 32)
	h.mu.Lock()
	if len(h.rooms) >= h.cfg.MaxRooms {
		h.mu.Unlock()
		h.fail(c, "Server is full of rooms")
		return
	}
	if c.key != "" {
		n := 0
		for _, r := range h.rooms {
			if r.owner == c.key {
				n++
			}
		}
		if n >= h.cfg.RoomsPerKey {
			h.mu.Unlock()
			h.fail(c, "You already own the maximum number of rooms; delete one first")
			return
		}
	}
	for _, r := range h.rooms {
		if strings.EqualFold(r.name, name) {
			h.mu.Unlock()
			h.fail(c, "A room with that name exists")
			return
		}
	}
	id := randID()
	for h.rooms[id] != nil {
		id = randID()
	}
	now := time.Now()
	r := &room{
		hub: h, id: id, name: name, pass: pass, owner: c.key, seed: m.Seed, max: max,
		simVer: m.Sim, created: now, active: now, speed: 1, paused: false,
		players: map[uint32]*client{}, quit: make(chan struct{}), dirty: !m.Base,
		pendingBase: m.Base, creator: c.id,
	}
	h.rooms[id] = r
	h.mu.Unlock()
	go r.run()
	log.Printf("room %s %q created by %q seed=%d", id, name, c.name, m.Seed)
	r.mu.Lock()
	r.add(c)
	r.mu.Unlock()
}

func (h *hub) join(c *client, m *inbound) {
	if c.room != nil {
		h.fail(c, "Leave your current room first")
		return
	}
	h.mu.Lock()
	r := h.rooms[m.Room]
	h.mu.Unlock()
	if r == nil {
		h.fail(c, "That room no longer exists")
		return
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	switch {
	case r.pendingBase:
		h.fail(c, "Room is still being set up")
	case r.pass != "" && m.Pass != r.pass:
		h.fail(c, "Wrong room password")
	case len(r.players) >= r.max:
		h.fail(c, "Room is full")
	case m.Sim != "" && r.simVer != "" && m.Sim != r.simVer:
		h.fail(c, "Game version mismatch: room runs "+r.simVer+", you have "+m.Sim)
	default:
		r.add(c)
		log.Printf("room %s: %q joined (%d players)", r.id, c.name, len(r.players))
	}
}

func (h *hub) deleteRoom(c *client, m *inbound) {
	h.mu.Lock()
	r := h.rooms[m.Room]
	h.mu.Unlock()
	if r == nil {
		return
	}
	r.mu.Lock()
	ok := c.key != "" && r.owner == c.key && len(r.players) == 0
	r.mu.Unlock()
	if !ok {
		h.fail(c, "Only the owner can delete an empty room")
		return
	}
	h.removeRoom(r)
	log.Printf("room %s deleted by %q", r.id, c.name)
	c.send(jsonFrame(map[string]any{"t": "rooms", "rooms": h.roomList()}))
}

func (h *hub) removeRoom(r *room) {
	h.mu.Lock()
	delete(h.rooms, r.id)
	h.mu.Unlock()
	close(r.quit)
	h.deleteFiles(r.id)
}
