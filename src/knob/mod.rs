use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{freq, pl};

pub mod cpu;

pub struct PerfWrite {
    pub path: PathBuf,
    pub value: String,
    refusal: Option<&'static str>,
}

impl PerfWrite {
    pub fn new(path: impl Into<PathBuf>, value: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: value.into(),
            refusal: None,
        }
    }

    pub fn refusal(mut self, reason: &'static str) -> Self {
        self.refusal = Some(reason);
        self
    }

    pub fn apply(&self) -> Result<()> {
        fs::write(&self.path, &self.value).map_err(|e| match (e.kind(), self.refusal) {
            (
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::InvalidInput,
                Some(reason),
            ) => anyhow::anyhow!("kernel refused: {reason}"),
            _ => anyhow::Error::new(e).context(format!("write {}", self.path.display())),
        })
    }
}

pub fn write_all(writes: &[PerfWrite]) -> Result<()> {
    writes.iter().try_for_each(PerfWrite::apply)
}

pub trait Knob {
    fn name(&self) -> &'static str;
    fn perf_writes(&self) -> Result<Vec<PerfWrite>>;
}

#[derive(Default, Clone)]
pub struct Tuning {
    pub governor: Option<bool>,
    pub turbo: Option<bool>,
    pub max_khz: Option<u64>,
    pub power: [Option<u64>; pl::Limit::ALL.len()],
}

impl Tuning {
    pub fn knobs(&self) -> Vec<Box<dyn Knob>> {
        let mut knobs: Vec<Box<dyn Knob>> = Vec::new();
        if self.governor.unwrap_or(false) {
            knobs.push(Box::new(cpu::Governor));
        }
        if let Some(on) = self.turbo {
            knobs.push(Box::new(cpu::Turbo(on)));
        }
        if let Some(khz) = self.max_khz {
            knobs.push(Box::new(cpu::Freq(khz)));
        }
        for limit in pl::Limit::ALL {
            if let Some(uw) = self.power[limit as usize] {
                knobs.push(Box::new(cpu::Power(limit, uw)));
            }
        }
        knobs
    }

    pub fn over(&self, base: &Tuning) -> Tuning {
        let mut power = self.power;
        for limit in pl::Limit::ALL {
            power[limit as usize] = power[limit as usize].or(base.power[limit as usize]);
        }
        Tuning {
            governor: self.governor.or(base.governor),
            turbo: self.turbo.or(base.turbo),
            max_khz: self.max_khz.or(base.max_khz),
            power,
        }
    }

    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = Vec::new();
        if self.governor.unwrap_or(false) {
            rows.push(("scaling_governor", "performance".to_string()));
            rows.push(("energy_performance_preference", "performance".to_string()));
        }
        if let Some(on) = self.turbo {
            rows.push(("turbo", if on { "on" } else { "off" }.to_string()));
        }
        if let Some(khz) = self.max_khz {
            rows.push(("freq", format!("max {}", freq::fmt(khz))));
        }
        for limit in pl::Limit::ALL {
            if let Some(uw) = self.power[limit as usize] {
                rows.push((limit.label(), pl::fmt(uw)));
            }
        }
        rows
    }

    pub fn is_empty(&self) -> bool {
        !self.governor.unwrap_or(false)
            && self.turbo.is_none()
            && self.max_khz.is_none()
            && self.power.iter().all(Option::is_none)
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Snapshot(pub BTreeMap<PathBuf, String>);

impl Snapshot {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn engage(knobs: &[Box<dyn Knob>]) -> Result<Snapshot> {
    let mut saved = BTreeMap::new();
    for knob in knobs {
        let writes = knob
            .perf_writes()
            .with_context(|| format!("knob '{}'", knob.name()))?;
        for w in writes {
            let old = match fs::read_to_string(&w.path) {
                Ok(s) => s.trim().to_string(),
                Err(e) => {
                    skip(&w.path, "read", &e);
                    continue;
                }
            };
            if old == w.value {
                continue;
            }
            match w.apply() {
                Ok(()) => {
                    saved.insert(w.path, old);
                }
                Err(e) => skip(&w.path, "write", &e),
            }
        }
    }
    Ok(Snapshot(saved))
}

pub fn restore(snap: &Snapshot) {
    for (path, old) in &snap.0 {
        if let Err(e) = fs::write(path, old) {
            skip(path, "restore", &e);
        }
    }
}

fn skip(path: &Path, op: &str, err: &dyn Display) {
    eprintln!("minazuki: {op} skipped for {}: {err}", path.display());
}
