//! Multiplayer client. The server (see `server/`) is a relay: it orders player commands into
//! fixed turns and forwards them. Every client runs the real deterministic sim, applies each
//! turn's commands in order and then steps the turn's ticks, so all clients stay identical.
//! Clients upload periodic snapshots (late joiners start from one) and compare state hashes.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use serde_json::{Value, json};
use sim::{Command, Sim};

use crate::audio::{Sfx, SfxQueue};
use crate::menu::{MenuAction, MenuQueue, MenuState, Screen};
use crate::state::*;
use crate::time_control::Speed;

pub const PROTO: u32 = 1;
pub const DEFAULT_SERVER: &str = "76.179.76.99:4001";
/// Clients only play together when their simulations are identical. Bump on any sim change.
pub const SIM_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "-r1");

const HASH_EVERY: u64 = 50;
const ACK_EVERY: Duration = Duration::from_millis(250);
const PING_EVERY: Duration = Duration::from_secs(15);
const ROOMS_EVERY: Duration = Duration::from_secs(4);
const STEP_BUDGET: Duration = Duration::from_millis(12);

const KIND_JSON: u8 = 0;
const KIND_SNAPSHOT: u8 = 1;
const MAX_JSON: usize = 16 << 20;
const MAX_SNAPSHOT: usize = 128 << 20;

// ---------------------------------------------------------------------------------------
// Wire format: u32 BE length (kind byte + payload) | u8 kind | payload
// ---------------------------------------------------------------------------------------

pub fn encode_frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.extend_from_slice(&(payload.len() as u32 + 1).to_be_bytes());
    out.push(kind);
    out.extend_from_slice(payload);
    out
}

pub fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 5];
    r.read_exact(&mut hdr)?;
    let len = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
    let kind = hdr[4];
    let limit = match kind {
        KIND_JSON => MAX_JSON,
        KIND_SNAPSHOT => MAX_SNAPSHOT,
        _ => return Err(io::Error::other("bad frame kind")),
    };
    if len == 0 || len - 1 > limit {
        return Err(io::Error::other("bad frame length"));
    }
    let mut buf = vec![0u8; len - 1];
    r.read_exact(&mut buf)?;
    Ok((kind, buf))
}

fn json_frame(v: &Value) -> Vec<u8> {
    encode_frame(KIND_JSON, v.to_string().as_bytes())
}

pub fn snapshot_frame(turn: u64, tick: u64, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(16 + data.len());
    p.extend_from_slice(&turn.to_be_bytes());
    p.extend_from_slice(&tick.to_be_bytes());
    p.extend_from_slice(data);
    encode_frame(KIND_SNAPSHOT, &p)
}

// ---------------------------------------------------------------------------------------
// Connection: one reader thread and one writer thread behind channels
// ---------------------------------------------------------------------------------------

enum Incoming {
    Json(Value),
    Snapshot(Vec<u8>),
    Closed(String),
}

struct Conn {
    tx: Sender<Vec<u8>>,
    rx: Mutex<Receiver<Incoming>>,
    sock: Arc<Mutex<Option<TcpStream>>>,
    closed: Arc<AtomicBool>,
}

impl Conn {
    fn open(addr: String, hello: Value) -> Conn {
        let (in_tx, rx) = mpsc::channel();
        let (tx, out_rx) = mpsc::channel::<Vec<u8>>();
        let sock = Arc::new(Mutex::new(None));
        let closed = Arc::new(AtomicBool::new(false));
        let (sock2, closed2) = (sock.clone(), closed.clone());
        std::thread::spawn(move || {
            let stream = match connect(&addr) {
                Ok(s) => s,
                Err(e) => {
                    let _ = in_tx.send(Incoming::Closed(format!("Could not connect: {e}")));
                    return;
                }
            };
            if closed2.load(Ordering::Relaxed) {
                return;
            }
            let _ = stream.set_nodelay(true);
            let (Ok(mut writer), Ok(shutdown)) = (stream.try_clone(), stream.try_clone()) else {
                let _ = in_tx.send(Incoming::Closed("Socket error".into()));
                return;
            };
            *sock2.lock().expect("socket lock") = Some(shutdown);
            std::thread::spawn(move || {
                for frame in out_rx {
                    if writer.write_all(&frame).is_err() {
                        break;
                    }
                }
            });
            let mut reader = stream;
            let _ = reader.write_all(&json_frame(&hello));
            loop {
                match read_frame(&mut reader) {
                    Ok((KIND_JSON, p)) => match serde_json::from_slice(&p) {
                        Ok(v) => {
                            if in_tx.send(Incoming::Json(v)).is_err() {
                                return;
                            }
                        }
                        Err(_) => continue,
                    },
                    Ok((_, p)) => {
                        // Snapshot: u64 turn | u64 tick | sim bytes.
                        if p.len() >= 16 && in_tx.send(Incoming::Snapshot(p[16..].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        let why = if closed2.load(Ordering::Relaxed) {
                            String::new()
                        } else if e.kind() == io::ErrorKind::UnexpectedEof {
                            "Server closed the connection".to_string()
                        } else {
                            format!("Connection lost: {e}")
                        };
                        let _ = in_tx.send(Incoming::Closed(why));
                        return;
                    }
                }
            }
        });
        Conn {
            tx,
            rx: Mutex::new(rx),
            sock,
            closed,
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        if let Some(s) = self.sock.lock().expect("socket lock").take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

fn connect(addr: &str) -> io::Result<TcpStream> {
    let mut last = io::Error::other("no address");
    for sa in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&sa, Duration::from_secs(6)) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

// ---------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub players: u32,
    pub max: u32,
    pub seed: u64,
    pub tick: u64,
    pub locked: bool,
    pub speed: u32,
    pub paused: bool,
}

impl RoomInfo {
    fn parse(v: &Value) -> Option<RoomInfo> {
        Some(RoomInfo {
            id: v["id"].as_str()?.to_string(),
            name: v["name"].as_str()?.to_string(),
            players: v["players"].as_u64()? as u32,
            max: v["max"].as_u64()? as u32,
            seed: v["seed"].as_u64()?,
            tick: v["tick"].as_u64().unwrap_or(0),
            locked: v["locked"].as_bool().unwrap_or(false),
            speed: v["speed"].as_u64().unwrap_or(1) as u32,
            paused: v["paused"].as_bool().unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Offline,
    Connecting,
    Lobby,
    InRoom,
}

enum Item {
    Turn {
        n: u64,
        ticks: u32,
        cmds: Vec<(u32, Command)>,
    },
    SnapReq(u64),
}

#[derive(Resource)]
pub struct Net {
    conn: Option<Conn>,
    pub phase: Phase,
    /// Last problem to show on the multiplayer page.
    pub error: Option<String>,
    pub rooms: Vec<RoomInfo>,
    pub room: Option<RoomInfo>,
    pub players: Vec<(u32, String)>,
    pub you: u32,
    pub speed: u32,
    pub paused: bool,
    pub max_speed: u32,
    /// Last turn applied to the local sim, and the newest turn at join time.
    pub turn: u64,
    head: u64,
    queue: VecDeque<Item>,
    /// Waiting for the snapshot frame that follows a `joined` message.
    awaiting_snapshot: Option<(u64, bool)>,
    /// Sim restored but the menu has not taken it yet (first join).
    resyncing: bool,
    last_ack: Instant,
    acked: u64,
    last_ping: Instant,
    last_rooms: Instant,
    /// Name this client connected with.
    pub name: String,
    /// A world being hosted: becomes the room's starting state once the room exists.
    base: Option<Sim>,
}

impl Default for Net {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            conn: None,
            phase: Phase::Offline,
            error: None,
            rooms: vec![],
            room: None,
            players: vec![],
            you: 0,
            speed: 1,
            paused: false,
            max_speed: 64,
            turn: 0,
            head: 0,
            queue: VecDeque::new(),
            awaiting_snapshot: None,
            resyncing: false,
            last_ack: now,
            acked: 0,
            last_ping: now,
            last_rooms: now,
            name: String::new(),
            base: None,
        }
    }
}

/// A sim built from a join, waiting for the menu layer to switch the game over to it.
#[derive(Resource, Default)]
pub struct MpHandoff(pub Option<Sim>);

impl Net {
    /// Playing on a server: local time controls and saving are replaced by the shared ones.
    pub fn in_game(&self) -> bool {
        self.phase == Phase::InRoom
    }

    pub fn connected(&self) -> bool {
        matches!(self.phase, Phase::Lobby | Phase::InRoom)
    }

    pub fn catching_up(&self) -> bool {
        self.in_game() && self.turn < self.head
    }

    pub fn head(&self) -> u64 {
        self.head
    }

    fn send(&self, v: Value) {
        if let Some(c) = &self.conn {
            let _ = c.tx.send(json_frame(&v));
        }
    }

    fn send_raw(&self, bytes: Vec<u8>) {
        if let Some(c) = &self.conn {
            let _ = c.tx.send(bytes);
        }
    }

    pub fn connect(&mut self, addr: &str, name: &str, key: &str) {
        self.disconnect();
        self.phase = Phase::Connecting;
        self.name = name.to_string();
        let hello = json!({
            "t": "hello", "proto": PROTO, "name": name, "key": key, "sim": SIM_VERSION,
        });
        self.conn = Some(Conn::open(addr.to_string(), hello));
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.base = None;
        self.phase = Phase::Offline;
        self.room = None;
        self.rooms.clear();
        self.players.clear();
        self.queue.clear();
        self.awaiting_snapshot = None;
        self.turn = 0;
        self.head = 0;
    }

    pub fn refresh(&self) {
        self.send(json!({"t": "rooms"}));
    }

    pub fn create(&self, name: &str, seed: u64, pass: &str, max: u32) {
        self.send(json!({
            "t": "create", "name": name, "seed": seed, "pass": pass, "max": max,
            "sim": SIM_VERSION,
        }));
    }

    /// Open an existing world (a save or the running game) as a new room.
    pub fn host(&mut self, sim: Sim, name: &str, pass: &str, max: u32) {
        self.send(json!({
            "t": "create", "name": name, "seed": sim.seed(), "pass": pass, "max": max,
            "sim": SIM_VERSION, "base": true,
        }));
        self.base = Some(sim);
    }

    pub fn join(&self, id: &str, pass: &str) {
        self.send(json!({"t": "join", "room": id, "pass": pass, "sim": SIM_VERSION}));
    }

    pub fn delete(&self, id: &str) {
        self.send(json!({"t": "delete", "room": id}));
    }

    pub fn chat(&self, text: &str) {
        self.send(json!({"t": "chat", "text": text}));
    }

    pub fn request_speed(&self, speed: u32) {
        self.send(json!({"t": "speed", "v": speed}));
        if self.paused {
            self.send(json!({"t": "pause", "on": false}));
        }
    }

    pub fn request_pause(&self, on: bool) {
        self.send(json!({"t": "pause", "on": on}));
    }

    fn resync(&mut self) {
        self.queue.clear();
        self.resyncing = true;
        self.send(json!({"t": "resync"}));
    }
}

pub struct NetPlugin;

/// Systems that advance the shared sim; readers of sim state run after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MpStep;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Net>()
            .init_resource::<MpHandoff>()
            .add_systems(
                Update,
                (net_poll, net_step, mp_sync_virt)
                    .chain()
                    .in_set(MpStep)
                    .after(crate::menu::MenuSystems),
            );
    }
}

/// Run condition: no server session, so the local sim clock is in charge.
pub fn offline(net: Res<Net>) -> bool {
    !net.in_game()
}

// ---------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------

fn parse_cmds(v: &Value) -> Vec<(u32, Command)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let p = e["p"].as_u64()? as u32;
                    let c = serde_json::from_value(e["c"].clone()).ok()?;
                    Some((p, c))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_players(v: &Value) -> Vec<(u32, String)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| Some((p["id"].as_u64()? as u32, p["name"].as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(bevy::ecs::system::SystemParam)]
struct PollCtx<'w> {
    sim: ResMut<'w, SimState>,
    log: ResMut<'w, MessageLog>,
    menu: ResMut<'w, MenuState>,
    queue: ResMut<'w, MenuQueue>,
    hand: ResMut<'w, MpHandoff>,
    speed: ResMut<'w, Speed>,
    screen: Res<'w, Screen>,
}

#[allow(clippy::too_many_arguments)]
fn net_poll(mut net: ResMut<Net>, mut cx: PollCtx) {
    let mut incoming = vec![];
    if let Some(c) = &net.conn {
        let rx = c.rx.lock().expect("rx lock");
        while let Ok(m) = rx.try_recv() {
            incoming.push(m);
        }
    }
    for m in incoming {
        match m {
            Incoming::Closed(why) => {
                let was_playing = net.in_game();
                net.disconnect();
                if !why.is_empty() {
                    net.error = Some(why.clone());
                    cx.menu.notice = Some(why.clone());
                }
                if was_playing && *cx.screen == Screen::Playing {
                    cx.queue.0.push(MenuAction::MpLost(why));
                }
                return;
            }
            Incoming::Snapshot(bytes) => {
                let Some((_, first)) = net.awaiting_snapshot.take() else {
                    continue;
                };
                match Sim::load(&bytes) {
                    Ok(sim) => install(&mut net, &mut cx, sim, first),
                    Err(e) => {
                        net.error = Some(format!("Bad snapshot from server: {e}"));
                        cx.queue.0.push(MenuAction::MpLost(format!("Bad snapshot: {e}")));
                    }
                }
            }
            Incoming::Json(v) => handle_json(&mut net, &mut cx, v),
        }
    }
    // Keep-alive and lobby refresh.
    let now = Instant::now();
    if net.connected() && now.duration_since(net.last_ping) > PING_EVERY {
        net.last_ping = now;
        net.send(json!({"t": "ping", "n": 0}));
    }
    if net.phase == Phase::Lobby
        && cx.menu.open
        && cx.menu.page == crate::menu::Page::Multiplayer
        && now.duration_since(net.last_rooms) > ROOMS_EVERY
    {
        net.last_rooms = now;
        net.refresh();
    }
}

fn install(net: &mut Net, cx: &mut PollCtx, sim: Sim, first: bool) {
    if first {
        cx.hand.0 = Some(sim);
        cx.queue.0.push(MenuAction::MpBegin);
    } else {
        cx.sim.0 = sim;
    }
    net.resyncing = false;
}

fn handle_json(net: &mut Net, cx: &mut PollCtx, v: Value) {
    match v["t"].as_str().unwrap_or("") {
        "welcome" => {
            net.phase = Phase::Lobby;
            net.error = None;
            net.you = v["id"].as_u64().unwrap_or(0) as u32;
            net.max_speed = v["max_speed"].as_u64().unwrap_or(64) as u32;
            net.rooms = rooms_of(&v);
        }
        "rooms" => net.rooms = rooms_of(&v),
        "left" => {
            net.phase = Phase::Lobby;
            net.refresh();
        }
        "error" => {
            let msg = v["msg"].as_str().unwrap_or("Server error").to_string();
            net.base = None;
            net.error = Some(msg.clone());
            if net.in_game() {
                cx.log.push(MsgKind::Warn, msg);
            } else {
                cx.menu.notice = Some(msg);
            }
        }
        "joined" => {
            let first = !net.in_game() || v["first"].as_bool().unwrap_or(true);
            net.you = v["you"].as_u64().unwrap_or(0) as u32;
            net.room = RoomInfo::parse(&v["room"]);
            net.players = parse_players(&v["players"]);
            net.max_speed = v["max_speed"].as_u64().unwrap_or(64) as u32;
            net.turn = v["snap_turn"].as_u64().unwrap_or(0);
            net.head = v["head"].as_u64().unwrap_or(0);
            net.queue.clear();
            net.phase = Phase::InRoom;
            if let Some(r) = &net.room {
                net.speed = r.speed;
                net.paused = r.paused;
                cx.speed.0 = r.speed;
            }
            let seed = net.room.as_ref().map(|r| r.seed).unwrap_or(1);
            if let Some(sim) = net.base.take() {
                // We created this room from an existing world: upload it as turn 0.
                match sim.save() {
                    Ok(bytes) => net.send_raw(snapshot_frame(0, sim.tick(), &bytes)),
                    Err(e) => cx.log.push(MsgKind::Error, format!("Could not upload world: {e}")),
                }
                net.turn = 0;
                net.head = 0;
                install(net, cx, sim, true);
            } else if v["has_snap"].as_bool().unwrap_or(false) {
                net.awaiting_snapshot = Some((seed, first));
            } else {
                install(net, cx, Sim::new(seed), first);
            }
        }
        "players" => net.players = parse_players(&v["players"]),
        "turn" => {
            // Turns that follow a snapshot are held in the queue until the restore finishes.
            if net.in_game() {
                let n = v["n"].as_u64().unwrap_or(0);
                net.queue.push_back(Item::Turn {
                    n,
                    ticks: v["ticks"].as_u64().unwrap_or(0) as u32,
                    cmds: parse_cmds(&v["cmds"]),
                });
            }
        }
        "snapreq" => {
            if let Some(n) = v["n"].as_u64() {
                net.queue.push_back(Item::SnapReq(n));
            }
        }
        "state" => {
            net.speed = v["speed"].as_u64().unwrap_or(1) as u32;
            net.paused = v["paused"].as_bool().unwrap_or(false);
            cx.speed.0 = net.speed;
        }
        "chat" => {
            let text = v["text"].as_str().unwrap_or("");
            let name = v["name"].as_str().unwrap_or("");
            if name.is_empty() {
                cx.log.push(MsgKind::Info, text.to_string());
            } else {
                cx.log.push(MsgKind::Good, format!("{name}: {text}"));
            }
        }
        "resync" => {
            cx.log.push(MsgKind::Warn, "Out of sync with the room: resyncing");
            net.resync();
        }
        _ => {}
    }
}

fn rooms_of(v: &Value) -> Vec<RoomInfo> {
    v["rooms"]
        .as_array()
        .map(|a| a.iter().filter_map(RoomInfo::parse).collect())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn net_step(
    mut net: ResMut<Net>,
    mut sim: ResMut<SimState>,
    mut cmds: ResMut<CommandQueue>,
    mut log: ResMut<MessageLog>,
    mut sfx: ResMut<SfxQueue>,
    screen: Res<Screen>,
) {
    if !net.in_game() {
        return;
    }
    // Local input goes to the server; it comes back ordered inside a turn.
    for c in cmds.0.drain(..) {
        if let Ok(v) = serde_json::to_value(&c) {
            net.send(json!({"t": "cmd", "c": v}));
        }
    }
    if *screen != Screen::Playing || net.awaiting_snapshot.is_some() || net.resyncing {
        return;
    }
    let start = Instant::now();
    let mut stepped = false;
    while let Some(item) = net.queue.pop_front() {
        match item {
            Item::Turn { n, ticks, cmds } => {
                if n != net.turn + 1 {
                    log.push(MsgKind::Warn, "Missed a turn: resyncing");
                    net.resync();
                    return;
                }
                for (p, cmd) in cmds {
                    let mine = p == net.you;
                    let sound = crate::sim_bridge::command_sound(&cmd);
                    match sim.0.apply(cmd) {
                        Ok(()) if mine => sfx.0.extend(sound),
                        Err(e) if mine && e != "Nothing to remove" => {
                            log.push(MsgKind::Error, e);
                            sfx.0.push(Sfx::Error);
                        }
                        _ => {}
                    }
                }
                for _ in 0..ticks {
                    sim.0.step();
                }
                net.turn = n;
                stepped = true;
                if n <= net.head {
                    // Replaying history: nobody needs those notifications again.
                    sim.0.take_events();
                } else if n % HASH_EVERY == 0 {
                    net.send(json!({"t": "hash", "n": n, "hash": sim.0.state_hash()}));
                }
            }
            Item::SnapReq(n) => {
                if n == net.turn {
                    match sim.0.save() {
                        Ok(bytes) => net.send_raw(snapshot_frame(n, sim.0.tick(), &bytes)),
                        Err(e) => log.push(MsgKind::Error, format!("Snapshot failed: {e}")),
                    }
                }
            }
        }
        if start.elapsed() > STEP_BUDGET {
            break;
        }
    }
    let now = Instant::now();
    if stepped && net.turn > net.acked && now.duration_since(net.last_ack) > ACK_EVERY {
        net.last_ack = now;
        net.acked = net.turn;
        net.send(json!({"t": "ack", "n": net.turn}));
    }
}

/// The server owns pause: keep the visual clock in step with it, menus included.
fn mp_sync_virt(net: Res<Net>, mut virt: ResMut<Time<Virtual>>) {
    if net.in_game() && net.paused != virt.is_paused() {
        if net.paused {
            virt.pause();
        } else {
            virt.unpause();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Dir, HOME, Placeable};

    #[test]
    fn frames_round_trip() {
        let f = encode_frame(KIND_JSON, b"{\"t\":\"x\"}");
        let (k, p) = read_frame(&mut f.as_slice()).expect("frame");
        assert_eq!((k, p.as_slice()), (KIND_JSON, b"{\"t\":\"x\"}".as_slice()));
        let s = snapshot_frame(7, 9, b"abc");
        let (k, p) = read_frame(&mut s.as_slice()).expect("frame");
        assert_eq!(k, KIND_SNAPSHOT);
        assert_eq!(u64::from_be_bytes(p[..8].try_into().unwrap()), 7);
        assert_eq!(&p[16..], b"abc");
    }

    #[test]
    fn oversized_or_unknown_frames_are_rejected() {
        assert!(read_frame(&mut [0u8, 0, 0, 0, 0].as_slice()).is_err());
        assert!(read_frame(&mut [0u8, 0, 0, 2, 9, 0].as_slice()).is_err());
    }

    #[test]
    fn commands_survive_the_wire() {
        let cmd = Command::Place {
            planet: HOME,
            x: -3,
            y: 12,
            what: Placeable::Belt(sim::BeltTier::Basic),
            dir: Dir::E,
        };
        let v = serde_json::to_value(&cmd).expect("serialise");
        let back: Command = serde_json::from_value(v).expect("deserialise");
        assert_eq!(cmd, back);
    }

    #[test]
    fn two_clients_stay_identical_through_turns() {
        // The relay's contract: same ordered commands + same tick counts => same state.
        let cmds = [
            Command::Place {
                planet: HOME,
                x: 5,
                y: 5,
                what: Placeable::Belt(sim::BeltTier::Basic),
                dir: Dir::N,
            },
            Command::Remove {
                planet: HOME,
                x: 5,
                y: 5,
            },
        ];
        let run = |wire: bool| {
            let mut s = Sim::new(3);
            for turn in 0..40 {
                for c in cmds.iter().filter(|_| turn % 7 == 0) {
                    let c = if wire {
                        serde_json::from_value(serde_json::to_value(c).unwrap()).unwrap()
                    } else {
                        c.clone()
                    };
                    let _ = s.apply(c);
                }
                for _ in 0..2 {
                    s.step();
                }
            }
            s.state_hash()
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn snapshot_restores_the_same_state() {
        let mut a = Sim::new(11);
        for _ in 0..200 {
            a.step();
        }
        let b = Sim::load(&a.save().expect("save")).expect("load");
        assert_eq!(a.state_hash(), b.state_hash());
        let (mut a, mut b) = (a, b);
        for _ in 0..200 {
            a.step();
            b.step();
        }
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
