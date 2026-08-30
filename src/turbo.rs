use crate::knob::{self, PerfWrite};
use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

const INTEL_NO_TURBO: &str = "/sys/devices/system/cpu/intel_pstate/no_turbo";
const CPUFREQ_BOOST: &str = "/sys/devices/system/cpu/cpufreq/boost";
const REFUSAL: &str = "turbo is disabled in BIOS/firmware, software can't override it";

enum Control {
    NoTurbo,
    Boost,
}

fn control() -> Option<Control> {
    if Path::new(INTEL_NO_TURBO).exists() {
        Some(Control::NoTurbo)
    } else if Path::new(CPUFREQ_BOOST).exists() {
        Some(Control::Boost)
    } else {
        None
    }
}

pub fn state() -> Option<bool> {
    match control()? {
        Control::NoTurbo => Some(read(INTEL_NO_TURBO)? == 0),
        Control::Boost => Some(read(CPUFREQ_BOOST)? == 1),
    }
}

pub fn writes(on: bool) -> Vec<PerfWrite> {
    let (path, value) = match control() {
        Some(Control::NoTurbo) => (INTEL_NO_TURBO, if on { "0" } else { "1" }),
        Some(Control::Boost) => (CPUFREQ_BOOST, if on { "1" } else { "0" }),
        None => return Vec::new(),
    };
    vec![PerfWrite::new(path, value).refusal(REFUSAL)]
}

pub fn set(on: bool) -> Result<()> {
    let writes = writes(on);
    ensure!(!writes.is_empty(), "no turbo control on this machine");
    knob::write_all(&writes)
}

pub fn parse(value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "on" | "true" | "1" | "enable" => Ok(true),
        "off" | "false" | "0" | "disable" => Ok(false),
        other => anyhow::bail!("expected on/off, got '{other}'"),
    }
}

fn read(path: &str) -> Option<u8> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
