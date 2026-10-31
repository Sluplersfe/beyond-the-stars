//! Save slots: one file per slot, a small JSON header followed by the MessagePack sim.
//!
//! Layout: `BTS1` | u32 LE header length | header JSON | sim bytes.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sim::Sim;

use crate::paths;

const MAGIC: &[u8; 4] = b"BTS1";
pub const AUTOSAVE: &str = "autosave";

/// Tutorial progress stored alongside a save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TutorialSave {
    /// Current step while the tutorial is running.
    pub step: Option<u32>,
    /// Whether it was ever finished or dismissed.
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveMeta {
    pub name: String,
    pub seed: u64,
    pub tick: u64,
    pub colonies: u32,
    pub population: u32,
    pub saved_unix: u64,
    #[serde(default)]
    pub tutorial: TutorialSave,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveEntry {
    pub slot: String,
    pub meta: SaveMeta,
}

impl SaveEntry {
    pub fn is_autosave(&self) -> bool {
        self.slot == AUTOSAVE
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Make a user-typed name safe to use as a file name.
pub fn slot_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_lowercase();
    if s.is_empty() { "colony".into() } else { s }
}

fn path_for(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.save"))
}

/// A slot name that is not taken yet, derived from `name`.
pub fn unique_slot(dir: &Path, name: &str) -> String {
    let base = slot_name(name);
    if !path_for(dir, &base).exists() {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|s| !path_for(dir, s).exists())
        .expect("unbounded range")
}

pub fn meta_for(sim: &Sim, name: &str, tutorial: TutorialSave) -> SaveMeta {
    SaveMeta {
        name: name.to_string(),
        seed: sim.seed(),
        tick: sim.tick(),
        colonies: sim.colonies().count() as u32,
        population: sim
            .colonies()
            .filter_map(|p| p.colony.as_ref())
            .map(|c| c.population)
            .sum(),
        saved_unix: now_unix(),
        tutorial,
    }
}

pub fn encode(sim: &Sim, meta: &SaveMeta) -> Result<Vec<u8>, String> {
    let header = serde_json::to_vec(meta).map_err(|e| e.to_string())?;
    let body = sim.save()?;
    let mut out = Vec::with_capacity(8 + header.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&body);
    Ok(out)
}

fn split(bytes: &[u8]) -> Result<(SaveMeta, &[u8]), String> {
    if bytes.len() < 8 || &bytes[..4] != MAGIC {
        return Err("not a Beyond the Stars save".into());
    }
    let n = u32::from_le_bytes(bytes[4..8].try_into().expect("4 bytes")) as usize;
    let header = bytes
        .get(8..8 + n)
        .ok_or_else(|| "truncated save header".to_string())?;
    let meta = serde_json::from_slice(header).map_err(|e| e.to_string())?;
    Ok((meta, &bytes[8 + n..]))
}

pub fn decode(bytes: &[u8]) -> Result<(SaveMeta, Sim), String> {
    let (meta, body) = split(bytes)?;
    Ok((meta, Sim::load(body)?))
}

pub fn write_to(dir: &Path, slot: &str, sim: &Sim, meta: &SaveMeta) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = encode(sim, meta)?;
    let path = path_for(dir, slot);
    let tmp = path.with_extension("save.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn read_from(dir: &Path, slot: &str) -> Result<(SaveMeta, Sim), String> {
    let bytes = std::fs::read(path_for(dir, slot)).map_err(|e| e.to_string())?;
    decode(&bytes)
}

pub fn delete_from(dir: &Path, slot: &str) -> Result<(), String> {
    std::fs::remove_file(path_for(dir, slot)).map_err(|e| e.to_string())
}

/// All readable saves, newest first. Unreadable files are skipped.
pub fn list_in(dir: &Path) -> Vec<SaveEntry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<SaveEntry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "save" {
                return None;
            }
            let slot = path.file_stem()?.to_string_lossy().to_string();
            // Only the header is needed: read just the first few hundred bytes.
            let mut f = std::fs::File::open(&path).ok()?;
            let mut head = vec![0u8; 8];
            std::io::Read::read_exact(&mut f, &mut head).ok()?;
            if &head[..4] != MAGIC {
                return None;
            }
            let n = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
            if n > 1 << 20 {
                return None;
            }
            let mut h = vec![0u8; n];
            std::io::Read::read_exact(&mut f, &mut h).ok()?;
            let meta: SaveMeta = serde_json::from_slice(&h).ok()?;
            Some(SaveEntry { slot, meta })
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.meta.saved_unix));
    out
}

pub fn list() -> Vec<SaveEntry> {
    list_in(&paths::saves_dir())
}

pub fn write(slot: &str, sim: &Sim, meta: &SaveMeta) -> Result<(), String> {
    write_to(&paths::saves_dir(), slot, sim, meta)
}

pub fn read(slot: &str) -> Result<(SaveMeta, Sim), String> {
    read_from(&paths::saves_dir(), slot)
}

pub fn delete(slot: &str) -> Result<(), String> {
    delete_from(&paths::saves_dir(), slot)
}

pub fn unique(name: &str) -> String {
    unique_slot(&paths::saves_dir(), name)
}

/// "3 min ago", "2 h ago", or a calendar date for older saves.
pub fn ago(unix: u64) -> String {
    let now = now_unix();
    let d = now.saturating_sub(unix);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ => {
            let (y, m, day) = civil_from_days((unix / 86_400) as i64);
            format!("{y}-{m:02}-{day:02}")
        }
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bts-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn slot_names_are_filesystem_safe() {
        assert_eq!(slot_name("  My Colony! "), "my-colony");
        assert_eq!(slot_name("../../etc"), "etc");
        assert_eq!(slot_name("***"), "colony");
    }

    #[test]
    fn round_trip_keeps_state_and_header() {
        let dir = tmp("roundtrip");
        let mut sim = Sim::new(7);
        for _ in 0..50 {
            sim.step();
        }
        let meta = meta_for(
            &sim,
            "Alpha",
            TutorialSave {
                step: Some(3),
                finished: false,
            },
        );
        write_to(&dir, "alpha", &sim, &meta).expect("write");
        let (m2, s2) = read_from(&dir, "alpha").expect("read");
        assert_eq!(m2, meta);
        assert_eq!(s2.state_hash(), sim.state_hash());
        let list = list_in(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].slot, "alpha");
        assert_eq!(list[0].meta.tutorial.step, Some(3));
        delete_from(&dir, "alpha").expect("delete");
        assert!(list_in(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slots_do_not_collide() {
        let dir = tmp("unique");
        let sim = Sim::new(1);
        let meta = meta_for(&sim, "x", TutorialSave::default());
        assert_eq!(unique_slot(&dir, "Base"), "base");
        write_to(&dir, "base", &sim, &meta).expect("write");
        assert_eq!(unique_slot(&dir, "Base"), "base-2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_is_rejected_not_listed() {
        let dir = tmp("garbage");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("bad.save"), b"nope nope nope").expect("write");
        assert!(list_in(&dir).is_empty());
        assert!(decode(b"BTS1\xff\xff\xff\xff").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dates_convert() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
