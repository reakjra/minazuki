use crate::game::Game;
use crate::knob::{self, Snapshot};
use crate::{proc, scx};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub const FILE: &str = "/run/minazuki/state.json";
const RESTORE_GRACE: Duration = Duration::from_secs(5);

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub snapshot: Snapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

pub fn save(state: &State) -> Result<()> {
    if let Some(dir) = Path::new(FILE).parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(FILE, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

pub fn load() -> Result<Option<State>> {
    let bytes = match fs::read(FILE) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(serde_json::from_slice(&bytes).ok())
}

pub fn clear() {
    fs::remove_file(FILE).ok();
}

pub fn exists() -> bool {
    Path::new(FILE).exists()
}

pub fn recover() -> bool {
    let Ok(Some(state)) = load() else {
        return false;
    };
    knob::restore(&state.snapshot);
    if state.scheduler.is_some() {
        let _ = scx::stop();
    }
    clear();
    true
}

pub enum Start {
    Engaged,
    Added,
    Known,
}

#[derive(Default)]
pub struct Engine {
    active: HashSet<u32>,
    snapshot: Option<Snapshot>,
    game: Option<String>,
    profile: Option<String>,
    scheduler: Option<String>,
    base_scheduler: Option<String>,
    scx_managed: bool,
    pending_off: Option<Instant>,
}

impl Engine {
    pub fn new(base_scheduler: Option<String>) -> Self {
        Self {
            base_scheduler,
            ..Self::default()
        }
    }

    pub fn on_game_start(&mut self, pid: u32, game: &Game) -> Start {
        self.pending_off = None;
        let newly = self.active.insert(pid);
        if self.snapshot.is_none() {
            self.engage(game);
            Start::Engaged
        } else if newly {
            Start::Added
        } else {
            Start::Known
        }
    }

    pub fn on_exit(&mut self, pid: u32) -> bool {
        if self.active.remove(&pid) {
            self.maybe_arm()
        } else {
            false
        }
    }

    pub fn prune_dead(&mut self) -> bool {
        let dead: Vec<u32> = self
            .active
            .iter()
            .copied()
            .filter(|&pid| !proc::alive(pid))
            .collect();
        if dead.is_empty() {
            return false;
        }
        for pid in dead {
            self.active.remove(&pid);
        }
        self.maybe_arm()
    }

    pub fn tick(&mut self) -> bool {
        match self.pending_off {
            Some(deadline) if Instant::now() >= deadline && self.active.is_empty() => {
                self.disengage();
                true
            }
            _ => false,
        }
    }

    pub fn shutdown(&mut self) {
        self.disengage();
        self.active.clear();
    }

    fn maybe_arm(&mut self) -> bool {
        if self.active.is_empty() && self.snapshot.is_some() && self.pending_off.is_none() {
            self.pending_off = Some(Instant::now() + RESTORE_GRACE);
            true
        } else {
            false
        }
    }

    fn engage(&mut self, game: &Game) {
        self.snapshot = Some(knob::engage(&game.tuning.knobs()).unwrap_or_default());
        self.game = Some(game.name.clone());
        self.profile = game.profile.clone();

        self.scheduler = None;
        self.scx_managed = false;
        if let Some(sched) = &game.scheduler
            && (self.base_scheduler.is_some() || scx::is_idle())
        {
            match scx::load(sched, scx::Mode::Gaming) {
                Ok(()) => {
                    self.scx_managed = true;
                    self.scheduler = Some(sched.clone());
                }
                Err(e) => eprintln!("minazuki: scx start failed: {e}"),
            }
        }
        self.persist();
    }

    fn persist(&self) {
        let state = State {
            snapshot: self.snapshot.clone().unwrap_or_default(),
            game: self.game.clone(),
            scheduler: self.scheduler.clone(),
            profile: self.profile.clone(),
        };
        let _ = save(&state);
    }

    fn disengage(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            knob::restore(&snapshot);
        }
        if self.scx_managed {
            let restored = match &self.base_scheduler {
                Some(base) => scx::load(base, scx::Mode::Auto),
                None => scx::stop(),
            };
            if let Err(e) = restored {
                eprintln!("minazuki: scx restore failed: {e}");
            }
            self.scx_managed = false;
        }
        self.game = None;
        self.profile = None;
        self.scheduler = None;
        self.pending_off = None;
        clear();
    }
}
