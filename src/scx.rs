use anyhow::{Context, Result};
use std::process::Command;

pub fn available() -> bool {
    Command::new("scxctl")
        .arg("get")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub fn is_idle() -> bool {
    match Command::new("scxctl").arg("get").output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .to_lowercase()
            .contains("no scx scheduler running"),
        Err(_) => false,
    }
}

pub fn start_gaming(scheduler: &str) -> Result<()> {
    run(&["start", "--sched", scheduler, "--mode", "gaming"])
}

pub fn stop() -> Result<()> {
    run(&["stop"])
}

fn run(args: &[&str]) -> Result<()> {
    let status = Command::new("scxctl")
        .args(args)
        .status()
        .context("run scxctl")?;
    if !status.success() {
        anyhow::bail!("scxctl {} failed ({status})", args.join(" "));
    }
    Ok(())
}
