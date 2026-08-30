use crate::cpufreq;
use crate::knob::{self, PerfWrite};
use anyhow::{Context, Result, bail, ensure};

const REFUSAL: &str = "the frequency cap is locked";

pub struct FreqInfo {
    pub max_khz: u64,
    pub hw_max_khz: u64,
}

fn peak(file: &str) -> Option<u64> {
    cpufreq::policies()
        .iter()
        .filter_map(|policy| cpufreq::read_khz(&policy.join(file)))
        .max()
}

pub fn info() -> Option<FreqInfo> {
    Some(FreqInfo {
        max_khz: peak("scaling_max_freq")?,
        hw_max_khz: peak("cpuinfo_max_freq")?,
    })
}

pub fn hw_max() -> Option<u64> {
    peak("cpuinfo_max_freq")
}

pub fn max_writes(khz: u64) -> Vec<PerfWrite> {
    cpufreq::policies()
        .into_iter()
        .map(|policy| {
            PerfWrite::new(policy.join("scaling_max_freq"), khz.to_string()).refusal(REFUSAL)
        })
        .collect()
}

pub fn set_max(khz: u64) -> Result<()> {
    let writes = max_writes(khz);
    ensure!(!writes.is_empty(), "no cpufreq policies on this machine");
    knob::write_all(&writes)
}

pub fn reset_max() -> Result<()> {
    for policy in cpufreq::policies() {
        if let Some(hw) = cpufreq::read_khz(&policy.join("cpuinfo_max_freq")) {
            let _ = std::fs::write(policy.join("scaling_max_freq"), hw.to_string());
        }
    }
    Ok(())
}

pub fn parse(input: &str) -> Result<u64> {
    let lower = input.trim().to_lowercase();
    let body = lower.strip_suffix("hz").unwrap_or(&lower);
    let (digits, mult): (&str, f64) = if let Some(n) = body.strip_suffix('g') {
        (n, 1_000_000.0)
    } else if let Some(n) = body.strip_suffix('m') {
        (n, 1_000.0)
    } else if let Some(n) = body.strip_suffix('k') {
        (n, 1.0)
    } else {
        let ghz_ish = body.contains('.') || body.parse::<f64>().map(|v| v <= 10.0).unwrap_or(false);
        (body, if ghz_ish { 1_000_000.0 } else { 1_000.0 })
    };
    let value: f64 = digits
        .trim()
        .parse()
        .with_context(|| format!("not a frequency: '{input}'"))?;
    if value <= 0.0 {
        bail!("frequency must be positive");
    }
    Ok((value * mult) as u64)
}

pub fn fmt(khz: u64) -> String {
    format!("{:.2} GHz", khz as f64 / 1_000_000.0)
}
