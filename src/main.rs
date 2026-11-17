mod config;
mod cpufreq;
mod freq;
mod game;
mod inotify;
mod knob;
mod pl;
mod proc;
mod scx;
mod state;
mod turbo;
mod ui;
mod watcher;

use anyhow::{Result, bail};
use knob::Tuning;
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("status");
    let value = args.get(1).map(String::as_str);
    println!();
    let result = match cmd {
        "status" => cmd_status(),
        "config" => cmd_config(),
        "engage" => cmd_engage(),
        "restore" => cmd_restore(),
        "watch" => cmd_watch(),
        "daemon" => cmd_daemon(),
        "cpu" => cmd_cpu(value, args.get(2).map(String::as_str)),
        other => {
            eprintln!("minazuki: unknown command '{other}'");
            eprintln!(
                "usage: minazuki [status|config|daemon|engage|restore|watch|cpu <turbo|freq|pl1|pl2> [value]]"
            );
            std::process::exit(2);
        }
    };
    println!();
    result
}

fn cmd_status() -> Result<()> {
    cmd_cpu_summary()?;
    row("scx scheduler", scx_state());
    match state::load()? {
        Some(st) => println!("\n{}", engaged_line(&st)),
        None => println!("\n{}", ui::dim(&ui::mark(ui::Mark::Idle))),
    }
    Ok(())
}

fn engaged_line(st: &state::State) -> String {
    let mut line = ui::good(&ui::mark(ui::Mark::Engaged));
    if let Some(game) = &st.game {
        line.push_str(&format!("  {}", ui::good(game)));
    }
    if let Some(scheduler) = &st.scheduler {
        line.push_str(&format!("  {}", ui::dim(&format!("via {scheduler}"))));
    }
    if let Some(profile) = &st.profile {
        line.push_str(&format!("  {} {}", ui::dim("profile:"), ui::good(profile)));
    }
    line
}

fn cmd_config() -> Result<()> {
    let config = config::Config::load()?;
    let path = config::path();
    println!("{}", ui::head(&path.display().to_string()));
    if !path.exists() {
        println!("  {}", ui::dim("(no file yet, showing built-in defaults)"));
    }

    if config.startup_scheduler.is_some() || !config.startup.is_empty() {
        println!("\n{}", ui::head("startup"));
        println!("  {}", ui::dim("applied once at boot, never restored"));
        if let Some(sched) = &config.startup_scheduler {
            row("scheduler", ui::good(sched));
        }
        print_rows(&config.startup);
    }

    println!("\n{}", ui::head("default"));
    print_profile(config.default_scheduler.as_deref(), &config.default_tuning);
    for rule in &config.rules {
        println!("\n{}", ui::head(&rule.path));
        print_profile(rule.scheduler.as_deref(), &rule.tuning);
    }
    Ok(())
}

fn print_profile(scheduler: Option<&str>, tuning: &Tuning) {
    match scheduler {
        Some(sched) => row("scheduler", ui::good(sched)),
        None => row("scheduler", ui::dim("(left alone)")),
    }
    print_rows(tuning);
}

fn print_rows(tuning: &Tuning) {
    for (label, value) in tuning.rows() {
        row(label, ui::good(&value));
    }
}

fn turbo_state() -> String {
    match turbo::state() {
        Some(true) => ui::good("on"),
        Some(false) => ui::dim("off"),
        None => ui::dim("n/a"),
    }
}

fn scx_state() -> String {
    match scx::status() {
        scx::Status::Running(scheduler) => ui::good(&scheduler),
        scx::Status::Idle => ui::dim("none"),
        scx::Status::Unavailable => ui::dim("n/a"),
    }
}

fn cmd_engage() -> Result<()> {
    require_root()?;
    if state::exists() {
        bail!("already engaged (state file exists); run `restore` first");
    }
    let config = config::Config::load()?;
    let snapshot = knob::engage(&config.default_tuning.knobs())?;
    if snapshot.is_empty() {
        println!(
            "{}",
            ui::dim("nothing to change (already at performance, or no applicable knobs)")
        );
        return Ok(());
    }
    let changed = snapshot.len();
    state::save(&state::State {
        snapshot,
        ..Default::default()
    })?;
    ui::done(
        &ui::mark(ui::Mark::Engaged),
        &format!("(changed {changed} setting(s))"),
    );
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
    ui::done(
        &ui::mark(ui::Mark::Restored),
        &format!("({} setting(s))", state.snapshot.len()),
    );
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
    let wants_scx = config.wants_scx();
    let startup_scheduler = config.startup_scheduler.clone();
    if !config.startup.is_empty() {
        let applied = knob::engage(&config.startup.knobs())?.len();
        println!(
            "{}",
            ui::dim(&format!("startup: applied {applied} setting(s)"))
        );
    }
    let mut matcher = game::Matcher::new(config);

    let scx_ready = scx::available();
    if wants_scx && !scx_ready {
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
    let base_scheduler = startup_scheduler
        .filter(|_| scx_ready)
        .and_then(start_base_scheduler);
    let mut engine = state::Engine::new(base_scheduler);
    adopt_running(&matcher, &mut engine);

    let watcher = watcher::Watcher::new()?;
    let config_watch = match inotify::Watch::new(&config::path()) {
        Ok(watch) => Some(watch),
        Err(e) => {
            eprintln!("minazuki: config watch unavailable ({e}), use `systemctl reload minazuki`");
            None
        }
    };
    install_signal_handlers();
    println!("{}", ui::head("minazuki daemon running"));
    while !STOP.load(Ordering::SeqCst) {
        let edited = config_watch.as_ref().is_some_and(inotify::Watch::touched);
        if RELOAD.swap(false, Ordering::SeqCst) || edited {
            match config::Config::load() {
                Ok(cfg) => {
                    matcher = game::Matcher::new(cfg);
                    println!("{}", ui::dim("config reloaded"));
                    adopt_running(&matcher, &mut engine);
                }
                Err(e) => eprintln!("minazuki: config reload failed: {e}"),
            }
        }
        for event in watcher.recv()? {
            match event {
                watcher::Event::Exec { pid } => {
                    if let Some(game) = matcher.detect(pid) {
                        match engine.on_game_start(pid, &game) {
                            state::Start::Engaged => {
                                log_game(&game.name, pid);
                                match &game.scheduler {
                                    Some(sched) => ui::done(
                                        &ui::mark(ui::Mark::Engaged),
                                        &format!("(scheduler {sched})"),
                                    ),
                                    None => println!("{}", ui::good(&ui::mark(ui::Mark::Engaged))),
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
            ui::done(&ui::mark(ui::Mark::Restored), "(games closed)");
        }
    }
    engine.shutdown();
    println!("{}", ui::dim("stopped, cpu restored"));
    Ok(())
}

fn start_base_scheduler(scheduler: String) -> Option<String> {
    match scx::load(&scheduler, scx::Mode::Auto) {
        Ok(()) => {
            println!("{}", ui::dim(&format!("startup: scheduler {scheduler}")));
            Some(scheduler)
        }
        Err(e) => {
            eprintln!("minazuki: startup scheduler failed: {e}");
            None
        }
    }
}

fn adopt_running(matcher: &game::Matcher, engine: &mut state::Engine) {
    for pid in proc::pids() {
        if let Some(game) = matcher.detect(pid)
            && let state::Start::Engaged = engine.on_game_start(pid, &game)
        {
            ui::done(
                &ui::mark(ui::Mark::Engaged),
                &format!("({} already running)", game.name),
            );
        }
    }
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
        Some("pl1") => cmd_power(pl::Limit::Long, value),
        Some("pl2") => cmd_power(pl::Limit::Short, value),
        Some(other) => bail!("unknown cpu control '{other}' (try: turbo, freq, pl1, pl2)"),
    }
}

fn cmd_cpu_summary() -> Result<()> {
    let target = config::Config::load()
        .ok()
        .filter(|c| c.default_tuning.governor.unwrap_or(false))
        .map(|_| "performance");
    println!("{}", ui::head("cpu"));
    row("turbo", turbo_state());
    match freq::info() {
        Some(f) => row(
            "freq",
            format!(
                "{} {}",
                ui::good(&format!("max {}", freq::fmt(f.max_khz))),
                ui::dim(&format!("(hw ceiling {})", freq::fmt(f.hw_max_khz))),
            ),
        ),
        None => row("freq", ui::dim("n/a")),
    }
    for limit in pl::Limit::ALL {
        let state = match pl::info(limit) {
            Some(cap) => ui::good(&pl::fmt(cap.uw)),
            None => ui::dim("n/a"),
        };
        row(limit.label(), state);
    }
    row("scaling_governor", tallied("scaling_governor", target));
    row(
        "energy_performance_preference",
        tallied("energy_performance_preference", target),
    );
    Ok(())
}

fn row(label: &str, value: String) {
    println!("  {label:<31} {value}");
}

fn tallied(file: &str, target: Option<&str>) -> String {
    let groups = cpufreq::tally(file);
    if groups.is_empty() {
        return ui::dim("n/a");
    }
    groups
        .iter()
        .map(|(value, count)| {
            let body = match target {
                Some(target) if target != value => {
                    format!("{} {} {}", ui::dim(value), ui::dim("→"), ui::good(target))
                }
                Some(_) => format!("{}  {}", ui::good(value), ui::dim("(already set)")),
                None => ui::good(value),
            };
            match count {
                1 => body,
                n => format!("{body} {}", ui::dim(&format!("×{n}"))),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn cmd_power(limit: pl::Limit, value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        match pl::info(limit) {
            Some(cap) => println!(
                "{}: {}{}",
                limit.label(),
                ui::good(&pl::fmt(cap.uw)),
                cap.spec_uw.map_or_else(String::new, |spec| format!(
                    " {}",
                    ui::dim(&format!("(spec base {})", pl::fmt(spec)))
                )),
            ),
            None => println!(
                "{}",
                ui::dim(&format!("{}: no rapl on this machine", limit.label()))
            ),
        }
        return Ok(());
    };
    require_root()?;
    if value.eq_ignore_ascii_case("reset") {
        let spec = pl::reset(limit)?;
        ui::done(
            &format!("○ {}", limit.label()),
            &format!("reset to spec base power ({})", pl::fmt(spec)),
        );
        return Ok(());
    }
    let uw = pl::parse(value)?;
    pl::set(limit, uw)?;
    let actual = pl::info(limit).map_or(uw, |cap| cap.uw);
    let label = format!("● {}", limit.label());
    if actual == uw {
        ui::done(&label, &pl::fmt(actual));
    } else {
        ui::done(
            &label,
            &format!(
                "requested {}, firmware settled on {}",
                pl::fmt(uw),
                pl::fmt(actual)
            ),
        );
    }
    Ok(())
}

fn cmd_freq(value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        match freq::info() {
            Some(f) => println!(
                "freq: max {} (hw ceiling {})",
                ui::good(&freq::fmt(f.max_khz)),
                ui::dim(&freq::fmt(f.hw_max_khz)),
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
    let khz = freq::parse(value)?;
    freq::set_max(khz)?;
    let actual = freq::hw_max().map_or(khz, |hw| khz.min(hw));
    if actual < khz {
        ui::done(
            "● freq",
            &format!(
                "requested {}, clamped to {} (hw ceiling)",
                freq::fmt(khz),
                freq::fmt(actual)
            ),
        );
    } else {
        ui::done("● freq", &format!("max {}", freq::fmt(actual)));
    }
    Ok(())
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
    let on = turbo::parse(value)?;
    turbo::set(on)?;
    ui::done(
        if on { "● turbo" } else { "○ turbo" },
        if on { "on" } else { "off" },
    );
    Ok(())
}

fn require_root() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("needs root to write sysfs. try: sudo minazuki <cmd>");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engaged_line_tags_the_matched_profile() {
        let st = state::State {
            game: Some("Wukong.exe".into()),
            scheduler: Some("scx_lavd".into()),
            profile: Some("black myth - wukong".into()),
            ..Default::default()
        };
        assert_eq!(
            engaged_line(&st),
            format!(
                "{}  Wukong.exe  via scx_lavd  profile: black myth - wukong",
                ui::mark(ui::Mark::Engaged)
            )
        );
    }

    #[test]
    fn engaged_line_stays_short_on_the_default_profile() {
        let st = state::State {
            game: Some("cs2".into()),
            scheduler: Some("scx_cake".into()),
            ..Default::default()
        };
        assert_eq!(
            engaged_line(&st),
            format!("{}  cs2  via scx_cake", ui::mark(ui::Mark::Engaged))
        );
    }
}
