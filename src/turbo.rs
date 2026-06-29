use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const INTEL_NO_TURBO: &str = "/sys/devices/system/cpu/intel_pstate/no_turbo";
const CPUFREQ_BOOST: &str = "/sys/devices/system/cpu/cpufreq/boost";

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

pub fn set(on: bool) -> Result<()> {
    let result = match control().context("no turbo control on this machine")? {
        Control::NoTurbo => fs::write(INTEL_NO_TURBO, if on { "0" } else { "1" }),
        Control::Boost => fs::write(CPUFREQ_BOOST, if on { "1" } else { "0" }),
    };
    result.map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            anyhow::anyhow!(
                "kernel refused: turbo is disabled in BIOS/firmware, software can't override it"
            )
        }
        _ => anyhow::Error::new(e).context("write turbo control"),
    })
}

fn read(path: &str) -> Option<u8> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
