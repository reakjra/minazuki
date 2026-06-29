use crate::config::Config;
use crate::proc;

const PROTON_PARENTS: &[&str] = &["wine", "reaper", "umu-run"];

pub struct Game {
    pub name: String,
    pub scheduler: Option<String>,
}

pub struct Matcher {
    game_names: Vec<String>,
    system_exes: Vec<String>,
    default_scheduler: Option<String>,
    scheduler_rules: Vec<(String, String)>,
}

impl Matcher {
    pub fn new(config: &Config) -> Self {
        Self {
            game_names: config.game_names.clone(),
            system_exes: config.system_exes.clone(),
            default_scheduler: config.default_scheduler.clone(),
            scheduler_rules: config.scheduler_rules.clone(),
        }
    }

    pub fn detect(&self, pid: u32) -> Option<Game> {
        let name = proc::name(pid)?;
        let lower = name.to_lowercase();
        let is_game = self.game_names.contains(&lower)
            || (proc::is_exe(name.as_bytes())
                && !self.system_exes.contains(&lower)
                && proton_parent(pid));
        if !is_game {
            return None;
        }
        Some(Game {
            name,
            scheduler: self.scheduler_for(pid),
        })
    }

    fn scheduler_for(&self, pid: u32) -> Option<String> {
        let path = proc::cmdline(pid).unwrap_or_default().to_lowercase();
        for (needle, scheduler) in &self.scheduler_rules {
            if path.contains(needle) {
                return Some(scheduler.clone());
            }
        }
        self.default_scheduler.clone()
    }
}

fn proton_parent(pid: u32) -> bool {
    let mut current = pid;
    for _ in 0..10 {
        if current <= 1 {
            return false;
        }
        let Some(parent) = proc::ppid(current) else {
            return false;
        };
        if parent <= 1 {
            return false;
        }
        if is_proton_process(parent) {
            return true;
        }
        current = parent;
    }
    false
}

fn is_proton_process(pid: u32) -> bool {
    if let Some(comm) = proc::comm(pid) {
        let comm = comm.to_lowercase();
        if PROTON_PARENTS.iter().any(|needle| comm.contains(needle)) {
            return true;
        }
    }
    proc::cmdline_contains(pid, "proton")
}
