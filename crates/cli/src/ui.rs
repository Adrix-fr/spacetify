//! Styled terminal output and keyboard input.
//!
//! Colours are dropped automatically when output is redirected or when
//! `NO_COLOR` is set.

use std::fmt::Display;
use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, StyledContent, Stylize};
use crossterm::{cursor, execute, queue, terminal};

pub const ACCENT: Color = Color::Rgb {
    r: 255,
    g: 61,
    b: 127,
};

static COLOR: AtomicBool = AtomicBool::new(true);

pub fn init() {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    COLOR.store(io::stdout().is_terminal() && !no_color, Ordering::Relaxed);
}

/// Applies `style` to `text`, or returns it as is when colours are off.
pub fn paint(
    text: impl Display,
    style: impl FnOnce(StyledContent<String>) -> StyledContent<String>,
) -> String {
    let text = text.to_string();
    if COLOR.load(Ordering::Relaxed) {
        style(text.stylize()).to_string()
    } else {
        text
    }
}

pub fn is_interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub fn title(text: &str) {
    println!("{}\n", paint(text, |s| s.with(ACCENT).bold()));
}

pub fn row(label: impl Display, value: impl Display) {
    println!(
        "  {}  {value}",
        paint(format!("{label:<12}"), |s| s.dark_grey())
    );
}

pub fn success(text: impl Display) {
    println!("{} {}", paint("✓", |s| s.green().bold()), indent(text));
}

pub fn warn(text: impl Display) {
    println!("{} {}", paint("!", |s| s.yellow().bold()), indent(text));
}

pub fn error(text: impl Display) {
    eprintln!("{} {}", paint("✗", |s| s.red().bold()), indent(text));
}

pub fn hint(text: impl Display) {
    println!("{}", paint(text, |s| s.dark_grey()));
}

/// Keeps continuation lines aligned with the text after the symbol.
fn indent(text: impl Display) -> String {
    text.to_string().replace('\n', "\n  ")
}

/// Asks a yes/no question. Answers on a single key press when possible.
pub fn confirm(question: &str) -> bool {
    print!("{} {question} ", paint("?", |s| s.with(ACCENT).bold()));
    let _ = io::stdout().flush();

    let yes = if is_interactive() {
        read_yes_key().unwrap_or(false)
    } else {
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer).is_ok()
            && matches!(
                answer.trim().to_lowercase().as_str(),
                "y" | "yes" | "o" | "oui"
            )
    };
    if is_interactive() {
        let answer = if yes {
            paint("✓", |s| s.green())
        } else {
            paint("✗", |s| s.red())
        };
        println!("{answer}");
    }
    yes
}

/// Asks for a line of text. Surrounding quotes are removed, since dragging a
/// folder into a terminal often adds them. `None` if the answer is empty.
pub fn ask_line(question: &str) -> Option<String> {
    print!("{} {question} ", paint("?", |s| s.with(ACCENT).bold()));
    let _ = io::stdout().flush();
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).ok()?;
    let answer = answer.trim().trim_matches(['"', '\'']).trim();
    (!answer.is_empty()).then(|| answer.to_owned())
}

fn read_yes_key() -> io::Result<bool> {
    let _raw = RawMode::enable()?;
    let key = next_key()?;
    Ok(matches!(key.code, KeyCode::Char('y' | 'Y' | 'o' | 'O')))
}

pub fn wait_for_key() -> io::Result<()> {
    let _raw = RawMode::enable()?;
    next_key().map(drop)
}

pub fn clear_screen() -> io::Result<()> {
    execute!(
        io::stdout(),
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )
}

/// Next key press, ignoring releases (reported on Windows).
pub fn next_key() -> io::Result<event::KeyEvent> {
    loop {
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(key);
        }
    }
}

pub fn is_ctrl_c(key: &event::KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Raw mode with a hidden cursor, restored on drop even after an error.
pub struct RawMode;

impl RawMode {
    pub fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        queue!(io::stdout(), cursor::Hide)?;
        io::stdout().flush()?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}
