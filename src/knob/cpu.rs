use super::{Knob, PerfWrite};
use crate::{cpufreq, freq, pl, turbo};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct Governor;
pub struct Turbo(pub bool);
pub struct Freq(pub u64);
pub struct Power(pub pl::Limit, pub u64);

impl Knob for Governor {
    fn name(&self) -> &'static str {
        "governor"
    }

    fn perf_writes(&self) -> Result<Vec<PerfWrite>> {
        let mut writes = Vec::new();
        for policy in cpufreq::policies() {
            let gov = policy.join("scaling_governor");
            if governor_available(&policy, "performance") {
                writes.push(PerfWrite::new(gov, "performance"));
            }
            let epp = policy.join("energy_performance_preference");
            if epp.exists() {
                writes.push(PerfWrite::new(epp, "performance"));
            }
        }
        Ok(writes)
    }
}

impl Knob for Turbo {
    fn name(&self) -> &'static str {
        "turbo"
    }

    fn perf_writes(&self) -> Result<Vec<PerfWrite>> {
        Ok(turbo::writes(self.0))
    }
}

impl Knob for Freq {
    fn name(&self) -> &'static str {
        "freq"
    }

    fn perf_writes(&self) -> Result<Vec<PerfWrite>> {
        Ok(freq::max_writes(self.0))
    }
}

impl Knob for Power {
    fn name(&self) -> &'static str {
        self.0.label()
    }

    fn perf_writes(&self) -> Result<Vec<PerfWrite>> {
        Ok(pl::writes(self.0, self.1))
    }
}

fn governor_available(policy: &Path, want: &str) -> bool {
    fs::read_to_string(policy.join("scaling_available_governors"))
        .map(|s| s.split_whitespace().any(|g| g == want))
        .unwrap_or(false)
}
