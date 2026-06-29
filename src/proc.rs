use std::fs;
use std::path::Path;

pub fn name(pid: u32) -> Option<String> {
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    select_name(&cmdline)
}

pub fn cmdline(pid: u32) -> Option<String> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let text = String::from_utf8_lossy(&bytes).replace('\0', " ");
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

pub fn comm(pid: u32) -> Option<String> {
    let comm = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(comm.trim().to_owned())
}

pub fn ppid(pid: u32) -> Option<u32> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("PPid:"))?;
    line[5..].trim().parse().ok()
}

pub fn cmdline_contains(pid: u32, needle: &str) -> bool {
    match fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).contains(needle),
        Err(_) => false,
    }
}

pub fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

pub fn pids() -> Vec<u32> {
    let Ok(dir) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.flatten()
        .filter_map(|entry| entry.file_name().to_str().and_then(|n| n.parse().ok()))
        .collect()
}

pub fn is_exe(name: &[u8]) -> bool {
    name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".exe")
}

fn select_name(cmdline: &[u8]) -> Option<String> {
    let mut fallback: Option<&[u8]> = None;
    for arg in cmdline.split(|&b| b == 0) {
        let base = basename(arg);
        if base.is_empty() {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(base);
        }
        if is_exe(base) {
            return Some(String::from_utf8_lossy(base).into_owned());
        }
    }
    fallback.map(|b| String::from_utf8_lossy(b).into_owned())
}

fn basename(path: &[u8]) -> &[u8] {
    let unix = path.iter().rposition(|&b| b == b'/');
    let windows = path.iter().rposition(|&b| b == b'\\');
    match unix.max(windows) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}
