// Beyond the Stars multiplayer relay.
//
// The game simulation is deterministic, so the server never runs it: it orders player
// commands into fixed turns, stores them with periodic client-uploaded snapshots, and
// relays everything. Clients run identical simulations and compare state hashes.
package main

import (
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"
)

func env(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}

func envInt(k string, def int) int {
	if v, err := strconv.Atoi(os.Getenv(k)); err == nil {
		return v
	}
	return def
}

func main() {
	health := flag.Bool("health", false, "dial the local server and exit 0 if it answers")
	flag.Parse()
	cfg := config{
		Addr:         env("BTS_ADDR", ":4001"),
		DataDir:      env("BTS_DATA", "/data"),
		ServerPass:   os.Getenv("BTS_SERVER_PASS"),
		MaxSpeed:     envInt("BTS_MAX_SPEED", 64),
		MaxRooms:     envInt("BTS_MAX_ROOMS", 40),
		MaxPerIP:     envInt("BTS_MAX_PER_IP", 8),
		MaxConns:     envInt("BTS_MAX_CONNS", 200),
		RoomsPerKey:  envInt("BTS_ROOMS_PER_KEY", 3),
		IdleRoomDays: envInt("BTS_IDLE_ROOM_DAYS", 30),
	}
	if *health {
		_, port, _ := net.SplitHostPort(cfg.Addr)
		conn, err := net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", port), 3*time.Second)
		if err != nil {
			os.Exit(1)
		}
		conn.Close()
		return
	}
	h := newHub(cfg)
	h.load()
	l, err := net.Listen("tcp", cfg.Addr)
	if err != nil {
		log.Fatal(err)
	}
	log.Printf("listening on %s (max speed %dx, data %s)", cfg.Addr, cfg.MaxSpeed, cfg.DataDir)
	go h.serve(l)

	tick := time.NewTicker(20 * time.Second)
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)
	for {
		select {
		case <-tick.C:
			h.saveDirty()
			h.expire()
		case <-stop:
			log.Printf("shutting down")
			l.Close()
			h.saveDirty()
			return
		}
	}
}
