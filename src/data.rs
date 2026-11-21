use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Track {
    pub f: String,
    pub n: String,
    #[serde(default)]
    pub a: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LogEntry {
    pub date: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub const TRACKS_JSON: &str = include_str!("../assets/tracks.json");
pub const LOG_JSON: &str = include_str!("../assets/updates.json");

pub const MOODS: [&str; 6] = ["all", "chill", "hype", "weird", "sleep", "sad"];

pub fn tracks() -> Vec<Track> {
    serde_json::from_str(TRACKS_JSON).unwrap_or_default()
}

pub fn log() -> Vec<LogEntry> {
    serde_json::from_str(LOG_JSON).unwrap_or_default()
}

pub fn matches_mood(t: &Track, mood: &str) -> bool {
    if mood == "all" {
        return true;
    }
    let keys: &[&str] = match mood {
        "chill" => &["chill", "coffee", "ambient", "drone", "grotto", "soft", "calm", "quiet"],
        "hype" => &["hype", "fast", "fight", "battle", "boss", "heavy", "metal", "dokuro"],
        "weird" => &["weird", "wacky", "cactus", "horror", "sinister", "lord", "verity", "foxy"],
        "sleep" => &["sleep", "dream", "night", "moon", "slow", "low", "napcast", "drone"],
        "sad" => &["sad", "miss", "remember", "downfall", "employment", "terrible", "meltdown"],
        _ => &[],
    };
    let hay = format!("{} {} {}", t.n, t.a.join(" "), t.f).to_lowercase();
    keys.iter().any(|k| hay.contains(k))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    #[serde(default = "default_volume")]
    pub volume: u8,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub loop_one: bool,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub plays: BTreeMap<String, u32>,
    #[serde(default)]
    pub last: BTreeMap<String, u64>,
}

fn default_volume() -> u8 { 70 }
fn default_speed() -> f32 { 1.0 }

impl Default for State {
    fn default() -> Self {
        Self {
            volume: 70,
            speed: 1.0,
            theme: "void".into(),
            shuffle: false,
            loop_one: false,
            favorites: Vec::new(),
            plays: BTreeMap::new(),
            last: BTreeMap::new(),
        }
    }
}

impl State {
    pub fn path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let d = PathBuf::from(appdata).join("void-records-native");
            let _ = fs::create_dir_all(&d);
            return d.join("state.json");
        }
        PathBuf::from("state.json")
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(Self::path(), s);
        }
    }

    pub fn fav_set(&self) -> HashSet<String> {
        self.favorites.iter().cloned().collect()
    }

    pub fn toggle_fav(&mut self, f: &str) {
        if let Some(i) = self.favorites.iter().position(|x| x == f) {
            self.favorites.remove(i);
        } else {
            self.favorites.push(f.to_string());
        }
    }
}

pub fn music_dir() -> PathBuf {
    std::env::var("VR_MUSIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"D:\Cube\void-records"))
}

pub fn fmt_time(t: std::time::Duration) -> String {
    let s = t.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}
