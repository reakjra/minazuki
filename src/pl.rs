use crate::knob::{self, PerfWrite};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::{Path, PathBuf};

const DIR: &str = "/sys/class/powercap";
const MAX_CONSTRAINTS: usize = 8;

#[derive(Clone, Copy)]
pub enum Limit {
    Long,
    Short,
}

impl Limit {
    pub const ALL: [Limit; 2] = [Limit::Long, Limit::Short];

    pub const fn label(self) -> &'static str {
        match self {
            Limit::Long => "pl1",
            Limit::Short => "pl2",
        }
    }

    const fn constraint(self) -> &'static str {
        match self {
            Limit::Long => "long_term",
            Limit::Short => "short_term",
        }
    }

    const fn refusal(self) -> &'static str {
        match self {
            Limit::Long => "firmware has the rapl pl1 limit locked",
            Limit::Short => "firmware has the rapl pl2 limit locked",
        }
    }
}

pub struct Cap {
    pub uw: u64,
    pub spec_uw: Option<u64>,
}

// TODO amd (genuinely how the fuck does intel have better RAPL than amd)
// also to note its read only for amd
fn package() -> Option<PathBuf> {
    fs::read_dir(DIR)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|p| read_trim(&p.join("name")).is_some_and(|n| n.starts_with("package")))
}

fn constraint(limit: Limit) -> Option<(PathBuf, usize)> {
    let zone = package()?;
    let index = (0..MAX_CONSTRAINTS).find(|i| {
        read_trim(&zone.join(format!("constraint_{i}_name")))
            .is_some_and(|name| name == limit.constraint())
    })?;
    Some((zone, index))
}

pub fn info(limit: Limit) -> Option<Cap> {
    let (zone, i) = constraint(limit)?;
    Some(Cap {
        uw: read_uw(&zone.join(format!("constraint_{i}_power_limit_uw")))?,
        spec_uw: read_uw(&zone.join(format!("constraint_{i}_max_power_uw"))).filter(|&uw| uw > 0),
    })
}

pub fn writes(limit: Limit, uw: u64) -> Vec<PerfWrite> {
    let Some((zone, i)) = constraint(limit) else {
        return Vec::new();
    };
    let path = zone.join(format!("constraint_{i}_power_limit_uw"));
    vec![PerfWrite::new(path, uw.to_string()).refusal(limit.refusal())]
}

pub fn set(limit: Limit, uw: u64) -> Result<()> {
    let writes = writes(limit, uw);
    ensure!(
        !writes.is_empty(),
        "no rapl {} constraint on this machine",
        limit.label()
    );
    knob::write_all(&writes)
}

pub fn reset(limit: Limit) -> Result<u64> {
    let spec = info(limit)
        .and_then(|cap| cap.spec_uw)
        .with_context(|| format!("no spec base power reported for {}", limit.label()))?;
    set(limit, spec)?;
    Ok(spec)
}

pub fn parse(input: &str) -> Result<u64> {
    let lower = input.trim().to_lowercase();
    let body = lower.strip_suffix('w').unwrap_or(&lower);
    let watts: f64 = body
        .trim()
        .parse()
        .with_context(|| format!("not a wattage: '{input}'"))?;
    if watts <= 0.0 {
        bail!("wattage must be positive");
    }
    Ok((watts * 1_000_000.0) as u64)
}

pub fn fmt(uw: u64) -> String {
    let watts = format!("{:.1}", uw as f64 / 1_000_000.0);
    format!("{} W", watts.strip_suffix(".0").unwrap_or(&watts))
}

fn read_trim(path: &Path) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_string())
}

fn read_uw(path: &Path) -> Option<u64> {
    read_trim(path)?.parse().ok()
}
