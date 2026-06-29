use super::{Knob, PerfWrite};
use crate::cpufreq;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct Cpu;

impl Knob for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
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

fn governor_available(policy: &Path, want: &str) -> bool {
    fs::read_to_string(policy.join("scaling_available_governors"))
        .map(|s| s.split_whitespace().any(|g| g == want))
        .unwrap_or(false)
}
