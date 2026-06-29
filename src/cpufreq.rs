use std::fs;
use std::path::{Path, PathBuf};

pub const DIR: &str = "/sys/devices/system/cpu/cpufreq";

pub fn policies() -> Vec<PathBuf> {
    let Ok(dir) = fs::read_dir(DIR) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("policy"))
        })
        .collect();
    out.sort();
    out
}

pub fn read_khz(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
