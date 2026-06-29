use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const SYSTEM_PROCESSES: &str = include_str!("../data/system-processes.toml");
const DEFAULT_CONFIG: &str = include_str!("../data/config.example.toml");
const DEFAULT_GAME_NAMES: &[&str] = &[
    "zenlesszonezero.exe",
    "genshinimpact.exe",
    "yuanshen.exe",
    "starrail.exe",
    "bh3.exe",
    "cs2",
    "dota2",
    "factorio",
]; // i mean, yeah. do i actually give a shit?

#[derive(Deserialize)]
struct SystemFile {
    system_processes: Vec<String>,
}

#[derive(Deserialize)]
struct UserConfig {
    #[serde(default)]
    games: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    scheduler: Option<String>,
    #[serde(default)]
    schedulers: HashMap<String, String>,
}

pub struct Config {
    pub system_exes: Vec<String>,
    pub game_names: Vec<String>,
    pub default_scheduler: Option<String>,
    pub scheduler_rules: Vec<(String, String)>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let system: SystemFile = toml::from_str(SYSTEM_PROCESSES)?;
        let mut system_exes = lowered(system.system_processes);
        let mut game_names: Vec<String> = DEFAULT_GAME_NAMES
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let mut default_scheduler = None;
        let mut scheduler_rules = Vec::new();

        let path = config_path();
        if let Ok(text) = fs::read_to_string(&path) {
            let user: UserConfig =
                toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
            game_names.extend(user.games.iter().map(|s| s.to_lowercase()));
            system_exes.extend(user.ignore.iter().map(|s| s.to_lowercase()));
            default_scheduler = user.scheduler;
            scheduler_rules = user
                .schedulers
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect();
        }

        Ok(Self {
            system_exes,
            game_names,
            default_scheduler,
            scheduler_rules,
        })
    }
}

pub fn ensure_default() -> Option<PathBuf> {
    let path = config_path();
    if path.exists() {
        return None;
    }
    fs::create_dir_all(path.parent()?).ok()?;
    fs::write(&path, DEFAULT_CONFIG).ok()?;
    Some(path)
}

fn config_path() -> PathBuf {
    std::env::var_os("MINAZUKI_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/minazuki/config.toml"))
}

fn lowered(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(|s| s.to_lowercase()).collect()
}
