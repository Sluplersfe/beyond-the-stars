package main

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
)

// Frame layout: u32 big-endian length (kind byte + payload) | u8 kind | payload.
const (
	kindJSON     byte = 0
	kindSnapshot byte = 1 // payload: u64 BE turn | u64 BE tick | sim bytes

	maxJSON     = 64 << 10
	maxSnapshot = 96 << 20
)

type frame struct {
	kind    byte
	payload []byte
}

func readFrame(r io.Reader) (frame, error) {
	var hdr [5]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return frame{}, err
	}
	n := binary.BigEndian.Uint32(hdr[:4])
	if n == 0 {
		return frame{}, errors.New("empty frame")
	}
	kind := hdr[4]
	n--
	limit := uint32(maxJSON)
	if kind == kindSnapshot {
		limit = maxSnapshot
	} else if kind != kindJSON {
		return frame{}, errors.New("bad frame kind")
	}
	if n > limit {
		return frame{}, errors.New("frame too large")
	}
	buf := make([]byte, n)
	if _, err := io.ReadFull(r, buf); err != nil {
		return frame{}, err
	}
	return frame{kind, buf}, nil
}

func encodeFrame(kind byte, payload []byte) []byte {
	out := make([]byte, 5+len(payload))
	binary.BigEndian.PutUint32(out[:4], uint32(len(payload)+1))
	out[4] = kind
	copy(out[5:], payload)
	return out
}

func jsonFrame(v any) []byte {
	b, err := json.Marshal(v)
	if err != nil {
		b = []byte(`{"t":"error","msg":"internal"}`)
	}
	return encodeFrame(kindJSON, b)
}

func snapshotFrame(turn, tick uint64, data []byte) []byte {
	p := make([]byte, 16+len(data))
	binary.BigEndian.PutUint64(p[:8], turn)
	binary.BigEndian.PutUint64(p[8:16], tick)
	copy(p[16:], data)
	return encodeFrame(kindSnapshot, p)
}

// inbound is every message a client may send; unused fields stay zero.
type inbound struct {
	T     string          `json:"t"`
	Proto int             `json:"proto,omitempty"`
	Sim   string          `json:"sim,omitempty"`
	Name  string          `json:"name,omitempty"`
	Key   string          `json:"key,omitempty"`
	Pass  string          `json:"pass,omitempty"`
	Room  string          `json:"room,omitempty"`
	Seed  uint64          `json:"seed,omitempty"`
	Max   int             `json:"max,omitempty"`
	C     json.RawMessage `json:"c,omitempty"`
	V     int             `json:"v,omitempty"`
	On    bool            `json:"on,omitempty"`
	Text  string          `json:"text,omitempty"`
	N     uint64          `json:"n,omitempty"`
	Hash  uint64          `json:"hash,omitempty"`
	Base  bool            `json:"base,omitempty"` // create: the creator will upload the starting snapshot
}

type roomInfo struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Players int    `json:"players"`
	Max     int    `json:"max"`
	Seed    uint64 `json:"seed"`
	Tick    uint64 `json:"tick"`
	Locked  bool   `json:"locked"`
	Speed   int    `json:"speed"`
	Paused  bool   `json:"paused"`
}

type playerInfo struct {
	ID   uint32 `json:"id"`
	Name string `json:"name"`
}
