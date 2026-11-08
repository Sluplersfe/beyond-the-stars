package main

import (
	"encoding/gob"
	"encoding/json"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"
)

func jsonUnmarshal(b []byte, v any) error { return json.Unmarshal(b, v) }

type roomFile struct {
	ID, Name, Pass, Owner, SimVer string
	Seed                          uint64
	Max, Speed                    int
	Paused                        bool
	Created, Active               time.Time
	Turn, Tick                    uint64
	Snap                          []byte
	SnapTurn, SnapTick            uint64
	Log                           []turnRec
}

func (h *hub) path(id string) string { return filepath.Join(h.cfg.DataDir, id+".room") }

func (h *hub) deleteFiles(id string) { os.Remove(h.path(id)) }

func (r *room) snapshotFile() roomFile {
	return roomFile{
		ID: r.id, Name: r.name, Pass: r.pass, Owner: r.owner, SimVer: r.simVer, Seed: r.seed,
		Max: r.max, Speed: r.speed, Paused: r.paused, Created: r.created, Active: r.active,
		Turn: r.turn, Tick: r.tick, Snap: r.snap, SnapTurn: r.snapTurn, SnapTick: r.snapTick,
		Log: append([]turnRec(nil), r.log...),
	}
}

func (h *hub) saveRoom(rf roomFile) error {
	tmp := h.path(rf.ID) + ".tmp"
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	if err := gob.NewEncoder(f).Encode(&rf); err != nil {
		f.Close()
		os.Remove(tmp)
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, h.path(rf.ID))
}

// saveDirty writes every changed room to disk.
func (h *hub) saveDirty() {
	h.mu.Lock()
	rooms := make([]*room, 0, len(h.rooms))
	for _, r := range h.rooms {
		rooms = append(rooms, r)
	}
	h.mu.Unlock()
	for _, r := range rooms {
		r.mu.Lock()
		if !r.dirty {
			r.mu.Unlock()
			continue
		}
		rf := r.snapshotFile()
		r.dirty = false
		r.mu.Unlock()
		if err := h.saveRoom(rf); err != nil {
			log.Printf("save room %s: %v", rf.ID, err)
			r.mu.Lock()
			r.dirty = true
			r.mu.Unlock()
		}
	}
}

func (h *hub) load() {
	if err := os.MkdirAll(h.cfg.DataDir, 0o755); err != nil {
		log.Fatalf("data dir: %v", err)
	}
	files, _ := filepath.Glob(filepath.Join(h.cfg.DataDir, "*.room"))
	for _, p := range files {
		f, err := os.Open(p)
		if err != nil {
			continue
		}
		var rf roomFile
		err = gob.NewDecoder(f).Decode(&rf)
		f.Close()
		if err != nil || rf.ID == "" || strings.ContainsAny(rf.ID, "/\\") {
			log.Printf("skipping unreadable room file %s: %v", p, err)
			continue
		}
		r := &room{
			hub: h, id: rf.ID, name: rf.Name, pass: rf.Pass, owner: rf.Owner, simVer: rf.SimVer,
			seed: rf.Seed, max: rf.Max, created: rf.Created, active: rf.Active,
			speed: rf.Speed, paused: rf.Paused, turn: rf.Turn, tick: rf.Tick,
			snap: rf.Snap, snapTurn: rf.SnapTurn, snapTick: rf.SnapTick, log: rf.Log,
			players: map[uint32]*client{}, quit: make(chan struct{}),
		}
		if r.speed < 1 || r.speed > h.maxSpeed {
			r.speed = 1
		}
		h.rooms[r.id] = r
		go r.run()
	}
	log.Printf("loaded %d rooms", len(h.rooms))
}

// expire deletes empty rooms nobody has touched for a long time.
func (h *hub) expire() {
	if h.cfg.IdleRoomDays <= 0 {
		return
	}
	cut := time.Now().Add(-time.Duration(h.cfg.IdleRoomDays) * 24 * time.Hour)
	h.mu.Lock()
	var old []*room
	for _, r := range h.rooms {
		r.mu.Lock()
		if len(r.players) == 0 && r.active.Before(cut) {
			old = append(old, r)
		}
		r.mu.Unlock()
	}
	h.mu.Unlock()
	for _, r := range old {
		log.Printf("room %s %q expired", r.id, r.name)
		h.removeRoom(r)
	}
}
