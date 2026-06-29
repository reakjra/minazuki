use crate::cpufreq;
use anyhow::{Context, Result};

pub struct FreqInfo {
    pub max_khz: u64,
    pub hw_max_khz: u64,
}

pub fn info() -> Option<FreqInfo> {
    let policy = cpufreq::policies().into_iter().next()?;
    Some(FreqInfo {
        max_khz: cpufreq::read_khz(&policy.join("scaling_max_freq"))?,
        hw_max_khz: cpufreq::read_khz(&policy.join("cpuinfo_max_freq"))?,
    })
}

pub fn set_max(khz: u64) -> Result<()> {
    let policies = cpufreq::policies();
    if policies.is_empty() {
        anyhow::bail!("no cpufreq policies on this machine");
    }
    for policy in policies {
        std::fs::write(policy.join("scaling_max_freq"), khz.to_string())
            .with_context(|| format!("write {}", policy.display()))?;
    }
    Ok(())
}

pub fn reset_max() -> Result<()> {
    for policy in cpufreq::policies() {
        if let Some(hw) = cpufreq::read_khz(&policy.join("cpuinfo_max_freq")) {
            let _ = std::fs::write(policy.join("scaling_max_freq"), hw.to_string());
        }
    }
    Ok(())
}
