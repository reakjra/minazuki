use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub mod cpu;

pub struct PerfWrite {
    pub path: std::path::PathBuf,
    pub value: String,
}

impl PerfWrite {
    pub fn new(path: impl Into<std::path::PathBuf>, value: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            value: value.into(),
        }
    }
}

pub trait Knob {
    fn name(&self) -> &'static str;
    fn perf_writes(&self) -> Result<Vec<PerfWrite>>;
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Snapshot(pub BTreeMap<std::path::PathBuf, String>);

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
            match fs::write(&w.path, &w.value) {
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

fn skip(path: &Path, op: &str, err: &std::io::Error) {
    eprintln!("minazuki: {op} skipped for {}: {err}", path.display());
}
