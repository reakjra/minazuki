use crate::knob::Tuning;
use crate::{freq, pl};
use anyhow::{Context, Result};
use serde::Deserialize;
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

#[derive(Deserialize, Default)]
struct Tweaks {
    #[serde(default)]
    cpu_governor: Option<bool>,
    #[serde(default)]
    cpu_turbo: Option<bool>,
    #[serde(default)]
    cpu_freq: Option<String>,
    #[serde(default)]
    cpu_pl1: Option<String>,
    #[serde(default)]
    cpu_pl2: Option<String>,
}

impl Tweaks {
    fn parse(&self) -> Result<Tuning> {
        Ok(Tuning {
            governor: self.cpu_governor,
            turbo: self.cpu_turbo,
            max_khz: parse_field(self.cpu_freq.as_deref(), "cpu_freq", freq::parse)?,
            power: [
                parse_field(self.cpu_pl1.as_deref(), "cpu_pl1", pl::parse)?,
                parse_field(self.cpu_pl2.as_deref(), "cpu_pl2", pl::parse)?,
            ],
        })
    }
}

fn parse_field<T>(
    value: Option<&str>,
    key: &str,
    parse: impl Fn(&str) -> Result<T>,
) -> Result<Option<T>> {
    value
        .map(|v| parse(v).with_context(|| format!("{key} = \"{v}\"")))
        .transpose()
}

#[derive(Deserialize, Default)]
struct Startup {
    #[serde(default)]
    scheduler: Option<String>,
    #[serde(default, flatten)]
    tweaks: Tweaks,
}

#[derive(Deserialize, Default)]
struct Profile {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    scheduler: Option<String>,
    #[serde(default, flatten)]
    tweaks: Tweaks,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserConfig {
    #[serde(default)]
    games: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    startup: Startup,
    #[serde(default)]
    defaults: Profile,
    #[serde(default)]
    process: Vec<Profile>,
}

pub struct Rule {
    pub path: String,
    pub scheduler: Option<String>,
    pub tuning: Tuning,
}

pub struct Config {
    pub system_exes: Vec<String>,
    pub game_names: Vec<String>,
    pub startup_scheduler: Option<String>,
    pub startup: Tuning,
    pub default_scheduler: Option<String>,
    pub default_tuning: Tuning,
    pub rules: Vec<Rule>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let system: SystemFile = toml::from_str(SYSTEM_PROCESSES)?;
        let mut config = Self {
            system_exes: lowered(system.system_processes),
            game_names: DEFAULT_GAME_NAMES
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            startup_scheduler: None,
            startup: Tuning::default(),
            default_scheduler: None,
            default_tuning: Tuning {
                governor: Some(true),
                ..Tuning::default()
            },
            rules: Vec::new(),
        };

        let path = path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(config);
        };
        let user: UserConfig =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        let blame = || format!("in {}", path.display());

        config.game_names.extend(lowered(user.games));
        config.system_exes.extend(lowered(user.ignore));
        config.startup_scheduler = user.startup.scheduler;
        config.startup = user.startup.tweaks.parse().with_context(blame)?;
        config.default_scheduler = user.defaults.scheduler;
        config.default_tuning = user
            .defaults
            .tweaks
            .parse()
            .with_context(blame)?
            .over(&config.default_tuning);

        for entry in user.process {
            let path = entry
                .path
                .with_context(|| format!("a [[process]] block is missing `path` ({})", blame()))?;
            let tuning = entry.tweaks.parse().with_context(blame)?;
            config.rules.push(Rule {
                path: path.to_lowercase(),
                scheduler: entry.scheduler.or_else(|| config.default_scheduler.clone()),
                tuning: tuning.over(&config.default_tuning),
            });
        }
        Ok(config)
    }

    pub fn wants_scx(&self) -> bool {
        self.startup_scheduler.is_some()
            || self.default_scheduler.is_some()
            || self.rules.iter().any(|r| r.scheduler.is_some())
    }
}

pub fn ensure_default() -> Option<PathBuf> {
    let path = path();
    if path.exists() {
        return None;
    }
    fs::create_dir_all(path.parent()?).ok()?;
    fs::write(&path, DEFAULT_CONFIG).ok()?;
    Some(path)
}

pub fn path() -> PathBuf {
    std::env::var_os("MINAZUKI_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/minazuki/config.toml"))
}

fn lowered(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(|s| s.to_lowercase()).collect()
}
