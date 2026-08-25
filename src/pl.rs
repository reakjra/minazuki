use anyhow::{Context, Result};
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

pub fn set(limit: Limit, uw: u64) -> Result<()> {
    let (zone, i) = constraint(limit)
        .with_context(|| format!("no rapl {} constraint on this machine", limit.label()))?;
    let path = zone.join(format!("constraint_{i}_power_limit_uw"));
    fs::write(&path, uw.to_string()).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::InvalidInput => {
            anyhow::anyhow!(
                "kernel refused: firmware has the rapl {} limit locked",
                limit.label()
            )
        }
        _ => anyhow::Error::new(e).context(format!("write {}", path.display())),
    })
}

pub fn reset(limit: Limit) -> Result<u64> {
    let spec = info(limit)
        .and_then(|cap| cap.spec_uw)
        .with_context(|| format!("no spec base power reported for {}", limit.label()))?;
    set(limit, spec)?;
    Ok(spec)
}

fn read_trim(path: &Path) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_string())
}

fn read_uw(path: &Path) -> Option<u64> {
    read_trim(path)?.parse().ok()
}
