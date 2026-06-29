mod config;
mod cpufreq;
mod freq;
mod game;
mod knob;
mod proc;
mod scx;
mod state;
mod turbo;
mod ui;
mod watcher;

use anyhow::{Context, Result, bail};
use knob::cpu::Cpu;
use knob::{Knob, PerfWrite};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);
static RELOAD: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        libc::SIGHUP => RELOAD.store(true, Ordering::SeqCst),
        _ => STOP.store(true, Ordering::SeqCst),
    }
}

fn install_signal_handlers() {
    let handler = on_signal as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGHUP, handler);
    }
}

fn knobs() -> Vec<Box<dyn Knob>> {
    vec![Box::new(Cpu)]
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    let value = args.get(1).map(String::as_str);
    println!();
    let result = match cmd {
        "status" => cmd_status(),
        "engage" => cmd_engage(),
        "restore" => cmd_restore(),
        "watch" => cmd_watch(),
        "daemon" => cmd_daemon(),
        "cpu" => cmd_cpu(value, args.get(2).map(String::as_str)),
        other => {
            eprintln!("minazuki: unknown command '{other}'");
            eprintln!(
                "usage: minazuki [status|daemon|engage|restore|watch|cpu <turbo|freq> [value]]"
            );
            std::process::exit(2);
        }
    };
    println!();
    result
}

fn cmd_status() -> Result<()> {
    for knob in knobs() {
        println!("{}", ui::head(knob.name()));
        let writes = knob.perf_writes()?;
        if writes.is_empty() {
            println!("  {}", ui::dim("(nothing applicable on this machine)"));
            continue;
        }
        for line in summarize(&writes) {
            println!("{line}");
        }
    }
    match state::load()? {
        Some(st) => {
            let mut line = format!("{} {}", ui::good("●"), ui::good("engaged"));
            if let Some(game) = &st.game {
                line.push_str(&format!("  {}", ui::good(game)));
            }
            if let Some(scheduler) = &st.scheduler {
                line.push_str(&format!("  {}", ui::dim(&format!("via {scheduler}"))));
            }
            println!("\n{line}");
        }
        None => println!("\n{} {}", ui::dim("○"), ui::dim("idle")),
    }
    Ok(())
}

fn summarize(writes: &[PerfWrite]) -> Vec<String> {
    let mut groups: Vec<(String, String, String, usize)> = Vec::new();
    for w in writes {
        let name = w.path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let cur = fs::read_to_string(&w.path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "?".into());
        match groups
            .iter_mut()
            .find(|g| g.0 == name && g.1 == cur && g.2 == w.value)
        {
            Some(g) => g.3 += 1,
            None => groups.push((name.to_string(), cur, w.value.clone(), 1)),
        }
    }
    groups
        .iter()
        .map(|(name, cur, target, count)| format_row(name, cur, target, *count))
        .collect()
}

fn format_row(name: &str, cur: &str, target: &str, count: usize) -> String {
    let tag = if count > 1 {
        ui::dim(&format!("  ×{count}"))
    } else {
        String::new()
    };
    let body = if cur == target {
        format!("{}  {}", ui::good(cur), ui::dim("(already set)"))
    } else {
        format!("{} {} {}", ui::dim(cur), ui::dim("→"), ui::good(target))
    };
    format!("  {name:<31} {body}{tag}")
}

fn cmd_engage() -> Result<()> {
    require_root()?;
    if state::exists() {
        bail!("already engaged (state file exists); run `restore` first");
    }
    let snapshot = knob::engage(&knobs())?;
    if snapshot.is_empty() {
        println!(
            "{}",
            ui::dim("nothing to change (already at performance, or no applicable knobs)")
        );
        return Ok(());
    }
    let changed = snapshot.len();
    state::save(&state::State { snapshot, ..Default::default() })?;
    ui::done("● engaged", &format!("(changed {changed} setting(s))"));
    Ok(())
}

fn cmd_restore() -> Result<()> {
    require_root()?;
    let Some(state) = state::load()? else {
        println!("{}", ui::dim("nothing to restore (no state file)"));
        return Ok(());
    };
    knob::restore(&state.snapshot);
    state::clear();
    ui::done("○ restored", &format!("({} setting(s))", state.snapshot.len()));
    Ok(())
}

fn cmd_watch() -> Result<()> {
    require_root()?;
    let watcher = watcher::Watcher::new()?;
    println!("{}", ui::head("watching proc events (ctrl-c to stop)"));
    loop {
        for event in watcher.recv()? {
            match event {
                watcher::Event::Exec { pid } => {
                    let name = proc::name(pid).unwrap_or_else(|| "?".into());
                    println!("{} {pid:<7} {name}", ui::good("exec"));
                }
                watcher::Event::Exit { pid } => {
                    println!("{}  {pid}", ui::dim("exit"));
                }
            }
        }
    }
}

fn cmd_daemon() -> Result<()> {
    require_root()?;
    if let Some(path) = config::ensure_default() {
        println!(
            "{}",
            ui::dim(&format!("wrote a starter config to {}", path.display()))
        );
    }
    let config = config::Config::load()?;
    let wants_scx = config.default_scheduler.is_some() || !config.scheduler_rules.is_empty();
    let mut matcher = game::Matcher::new(&config);
    let mut engine = state::Engine::new(knobs());

    if wants_scx && !scx::available() {
        println!(
            "{}",
            ui::dim(
                "scx: scheduler configured but scxctl/scx_loader unreachable, leaving scheduling alone"
            )
        );
    }
    if state::recover() {
        println!("{}", ui::dim("recovered stale state from a previous run"));
    }
    for pid in proc::pids() {
        if let Some(game) = matcher.detect(pid)
            && let state::Start::Engaged =
                engine.on_game_start(pid, &game.name, game.scheduler.as_deref())
        {
            ui::done("● engaged", &format!("({} already running)", game.name));
        }
    }

    let watcher = watcher::Watcher::new()?;
    install_signal_handlers();
    println!("{}", ui::head("minazuki daemon running"));
    while !STOP.load(Ordering::SeqCst) {
        if RELOAD.swap(false, Ordering::SeqCst) {
            match config::Config::load() {
                Ok(cfg) => {
                    matcher = game::Matcher::new(&cfg);
                    println!("{}", ui::dim("config reloaded"));
                }
                Err(e) => eprintln!("minazuki: config reload failed: {e}"),
            }
        }
        for event in watcher.recv()? {
            match event {
                watcher::Event::Exec { pid } => {
                    if let Some(game) = matcher.detect(pid) {
                        match engine.on_game_start(pid, &game.name, game.scheduler.as_deref()) {
                            state::Start::Engaged => {
                                log_game(&game.name, pid);
                                match &game.scheduler {
                                    Some(sched) => {
                                        ui::done("● engaged", &format!("(scheduler {sched})"))
                                    }
                                    None => println!("{}", ui::good("● engaged")),
                                }
                            }
                            state::Start::Added => log_game(&game.name, pid),
                            state::Start::Known => {}
                        }
                    }
                }
                watcher::Event::Exit { pid } => {
                    if engine.on_exit(pid) {
                        println!("{}", ui::dim("no games left, holding before restore"));
                    }
                }
            }
        }
        if engine.prune_dead() {
            println!("{}", ui::dim("no games left, holding before restore"));
        }
        if engine.tick() {
            ui::done("○ restored", "(games closed)");
        }
    }
    engine.shutdown();
    println!("{}", ui::dim("stopped, cpu restored"));
    Ok(())
}

fn log_game(name: &str, pid: u32) {
    println!(
        "{} {}",
        ui::good("game"),
        ui::dim(&format!("{name} ({pid})"))
    );
}

fn cmd_cpu(knob: Option<&str>, value: Option<&str>) -> Result<()> {
    match knob {
        None => cmd_cpu_summary(),
        Some("turbo") => cmd_turbo(value),
        Some("freq") => cmd_freq(value),
        Some(other) => bail!("unknown cpu control '{other}' (try: turbo, freq)"),
    }
}

fn cmd_cpu_summary() -> Result<()> {
    println!("{}", ui::head("cpu"));
    let turbo_state = match turbo::state() {
        Some(true) => ui::good("on"),
        Some(false) => ui::dim("off"),
        None => ui::dim("n/a"),
    };
    println!("  turbo   {turbo_state}");
    match freq::info() {
        Some(f) => println!(
            "  freq    {} {}",
            ui::good(&format!("max {}", fmt_freq(f.max_khz))),
            ui::dim(&format!("(hw ceiling {})", fmt_freq(f.hw_max_khz))),
        ),
        None => println!("  freq    {}", ui::dim("n/a")),
    }
    Ok(())
}

fn cmd_freq(value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        match freq::info() {
            Some(f) => println!(
                "freq: max {} (hw ceiling {})",
                ui::good(&fmt_freq(f.max_khz)),
                ui::dim(&fmt_freq(f.hw_max_khz)),
            ),
            None => println!("{}", ui::dim("freq: no cpufreq on this machine")),
        }
        return Ok(());
    };
    require_root()?;
    if value.eq_ignore_ascii_case("reset") {
        freq::reset_max()?;
        ui::done("○ freq", "reset to hardware max");
        return Ok(());
    }
    let khz = parse_khz(value)?;
    freq::set_max(khz)?;
    let actual = freq::info().map_or(khz, |f| f.max_khz);
    if actual < khz {
        ui::done(
            "● freq",
            &format!(
                "requested {}, clamped to {} (hw ceiling)",
                fmt_freq(khz),
                fmt_freq(actual)
            ),
        );
    } else {
        ui::done("● freq", &format!("max {}", fmt_freq(actual)));
    }
    Ok(())
}

fn parse_khz(input: &str) -> Result<u64> {
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

fn fmt_freq(khz: u64) -> String {
    format!("{:.2} GHz", khz as f64 / 1_000_000.0)
}

fn cmd_turbo(value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        match turbo::state() {
            Some(true) => println!("turbo: {}", ui::good("on")),
            Some(false) => println!("turbo: {}", ui::dim("off")),
            None => println!("{}", ui::dim("turbo: no control on this machine")),
        }
        return Ok(());
    };
    require_root()?;
    let on = parse_onoff(value)?;
    turbo::set(on)?;
    ui::done(
        if on { "● turbo" } else { "○ turbo" },
        if on { "on" } else { "off" },
    );
    Ok(())
}

fn parse_onoff(value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "on" | "true" | "1" | "enable" => Ok(true),
        "off" | "false" | "0" | "disable" => Ok(false),
        other => bail!("expected on/off, got '{other}'"),
    }
}

fn require_root() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("needs root to write sysfs. try: sudo minazuki <cmd>");
    }
    Ok(())
}
