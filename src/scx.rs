use anyhow::{Context, Result};
use std::process::Command;

pub enum Status {
    Unavailable,
    Idle,
    Running(String),
}

pub fn status() -> Status {
    let Ok(out) = Command::new("scxctl").arg("get").output() else {
        return Status::Unavailable;
    };
    if !out.status.success() {
        return Status::Unavailable;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if text.to_lowercase().contains("no scx scheduler running") {
        return Status::Idle;
    }
    match text.strip_prefix("running ") {
        Some(rest) => Status::Running(rest.to_owned()),
        None => Status::Running(text),
    }
}

pub fn available() -> bool {
    !matches!(status(), Status::Unavailable)
}

pub fn is_idle() -> bool {
    matches!(status(), Status::Idle)
}

#[derive(Clone, Copy)]
pub enum Mode {
    Auto,
    Gaming,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Gaming => "gaming",
        }
    }
}

pub fn load(scheduler: &str, mode: Mode) -> Result<()> {
    let verb = if is_idle() { "start" } else { "switch" };
    run(&[verb, "--sched", scheduler, "--mode", mode.as_str()])
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
