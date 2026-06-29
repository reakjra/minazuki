use std::io::IsTerminal;
use std::sync::OnceLock;

struct Style {
    truec: &'static str,
    ansi: &'static str,
}

const HEAD: Style = Style {
    truec: "38;2;186;172;255",
    ansi: "1;36",
};
const GOOD: Style = Style {
    truec: "38;2;152;224;178",
    ansi: "32",
};
const DIM: Style = Style {
    truec: "38;2;120;120;138",
    ansi: "2",
};

#[derive(Clone, Copy)]
enum Mode {
    Off,
    Term,
    True,
}

fn mode() -> Mode {
    static M: OnceLock<Mode> = OnceLock::new();
    *M.get_or_init(|| {
        if !std::io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some() {
            return Mode::Off;
        }
        match std::env::var("MINAZUKI_COLOR").as_deref() {
            Ok("term" | "ansi" | "terminal" | "16") => Mode::Term,
            _ => Mode::True,
        }
    })
}

fn emojis() -> bool {
    static E: OnceLock<bool> = OnceLock::new();
    *E.get_or_init(|| {
        matches!(
            std::env::var("MINAZUKI_EMOJIS").as_deref(),
            Ok("1" | "true" | "on" | "yes")
        )
    })
}

#[derive(Clone, Copy)]
pub enum Mark {
    Engaged,
    Restored,
    Idle,
}

impl Mark {
    const fn word(self) -> &'static str {
        match self {
            Mark::Engaged => "engaged",
            Mark::Restored => "restored",
            Mark::Idle => "idle",
        }
    }

    fn glyph(self) -> &'static str {
        match (emojis(), self) {
            (true, Mark::Engaged) => "🌸",
            (true, Mark::Restored | Mark::Idle) => "🍂",
            (false, Mark::Engaged) => "●",
            (false, Mark::Restored | Mark::Idle) => "○",
        }
    }
}

pub fn mark(m: Mark) -> String {
    format!("{} {}", m.glyph(), m.word())
}

fn paint(s: &str, style: &Style) -> String {
    let code = match mode() {
        Mode::Off => return s.to_string(),
        Mode::Term => style.ansi,
        Mode::True => style.truec,
    };
    format!("\x1b[{code}m{s}\x1b[0m")
}

pub fn head(s: &str) -> String {
    paint(s, &HEAD)
}
pub fn good(s: &str) -> String {
    paint(s, &GOOD)
}
pub fn dim(s: &str) -> String {
    paint(s, &DIM)
}

pub fn done(label: &str, detail: &str) {
    println!("{} {}", good(label), dim(detail));
}
