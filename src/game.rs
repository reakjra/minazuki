use crate::config::{Config, Rule};
use crate::knob::Tuning;
use crate::proc;

const PROTON_PARENTS: &[&str] = &["wine", "reaper", "umu-run"];

pub struct Game {
    pub name: String,
    pub profile: Option<String>,
    pub scheduler: Option<String>,
    pub tuning: Tuning,
}

pub struct Matcher {
    config: Config,
}

impl Matcher {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn detect(&self, pid: u32) -> Option<Game> {
        let name = proc::name(pid)?;
        let lower = name.to_lowercase();
        let rule = self.rule_for(pid);
        let launched_from_rule = rule.is_some_and(|rule| {
            proc::argv0(pid).is_some_and(|argv0| argv0.to_lowercase().contains(&rule.path))
        });
        let is_game = self.config.game_names.contains(&lower)
            || (!self.config.system_exes.contains(&lower)
                && (launched_from_rule || (proc::is_exe(name.as_bytes()) && proton_parent(pid))));
        if !is_game {
            return None;
        }
        let (profile, scheduler, tuning) = match rule {
            Some(rule) => (
                Some(rule.path.clone()),
                rule.scheduler.clone(),
                rule.tuning.clone(),
            ),
            None => (
                None,
                self.config.default_scheduler.clone(),
                self.config.default_tuning.clone(),
            ),
        };
        Some(Game {
            name,
            profile,
            scheduler,
            tuning,
        })
    }

    fn rule_for(&self, pid: u32) -> Option<&Rule> {
        if self.config.rules.is_empty() {
            return None;
        }
        let cmdline = proc::cmdline(pid)?.to_lowercase();
        self.config.rules.iter().find(|r| cmdline.contains(&r.path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Rule};
    use crate::knob::Tuning;
    use std::process::Command;
    use std::time::{Duration, Instant};

    fn matcher_for(path: &str) -> Matcher {
        Matcher::new(Config {
            system_exes: Vec::new(),
            game_names: Vec::new(),
            startup_scheduler: None,
            startup: Tuning::default(),
            default_scheduler: None,
            default_tuning: Tuning::default(),
            rules: vec![Rule {
                path: path.to_string(),
                scheduler: None,
                tuning: Tuning::default(),
            }],
        })
    }

    #[test]
    fn a_rule_matches_argv0_not_the_whole_command_line() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 5; : # zenless zone zero"])
            .spawn()
            .unwrap();
        let pid = child.id();

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if proc::cmdline(pid).is_some_and(|c| c.contains("zenless zone zero")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mentions_it = proc::cmdline(pid).is_some_and(|c| c.contains("zenless zone zero"));
        let detected = matcher_for("zenless zone zero").detect(pid).is_some();
        child.kill().ok();
        child.wait().ok();

        assert!(
            mentions_it,
            "the child should mention the game in its cmdline"
        );
        assert!(!detected, "but merely mentioning it is not being it");
    }
}
