//! A full-screen menu driven by the arrow keys, with sections.

use std::io::{self, Write};

use crossterm::event::KeyCode;
use crossterm::style::Stylize;
use crossterm::{cursor, queue, terminal};

use crate::ui::{self, ACCENT, RawMode, paint};

pub enum Entry {
    Section(String),
    Choice { label: String, hint: String },
}

impl Entry {
    pub fn section(name: impl Into<String>) -> Self {
        Self::Section(name.into())
    }

    pub fn choice(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Choice {
            label: label.into(),
            hint: hint.into(),
        }
    }
}

/// Shows the menu until the user picks a choice or quits.
///
/// Returns the position of the picked choice among choices only (sections
/// are not counted), or `None` on Esc, `q` or Ctrl+C.
pub fn select(header: &[String], entries: &[Entry], footer: &str) -> io::Result<Option<usize>> {
    let choices = entries
        .iter()
        .filter(|e| matches!(e, Entry::Choice { .. }))
        .count();
    if choices == 0 {
        return Ok(None);
    }
    let _raw = RawMode::enable()?;
    let mut current = 0;

    loop {
        draw(header, entries, current, footer)?;
        let key = ui::next_key()?;
        if ui::is_ctrl_c(&key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => current = (current + choices - 1) % choices,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => current = (current + 1) % choices,
            KeyCode::Home => current = 0,
            KeyCode::End => current = choices - 1,
            KeyCode::Enter | KeyCode::Char(' ') => return Ok(Some(current)),
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            _ => {}
        }
    }
}

fn draw(header: &[String], entries: &[Entry], current: usize, footer: &str) -> io::Result<()> {
    let mut lines = vec![String::new()];
    lines.extend(header.iter().cloned());

    let width = entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Choice { label, .. } => Some(label.chars().count()),
            Entry::Section(_) => None,
        })
        .max()
        .unwrap_or(0)
        + 3;

    let mut index = 0;
    for entry in entries {
        match entry {
            Entry::Section(name) => {
                lines.push(String::new());
                if !name.is_empty() {
                    lines.push(format!(
                        "  {}",
                        paint(name.to_uppercase(), |s| s.dark_grey().bold())
                    ));
                }
            }
            Entry::Choice { label, hint } => {
                let padded = format!("{label:<width$}");
                lines.push(if index == current {
                    format!(
                        "  {} {}{}",
                        paint("›", |s| s.with(ACCENT).bold()),
                        paint(padded, |s| s.with(ACCENT).bold()),
                        paint(hint, |s| s.grey())
                    )
                } else {
                    format!("    {padded}{}", paint(hint, |s| s.dark_grey()))
                });
                index += 1;
            }
        }
    }
    lines.push(String::new());
    lines.push(format!("  {}", paint(footer, |s| s.dark_grey())));

    let mut out = io::stdout();
    queue!(
        out,
        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All)
    )?;
    for line in lines {
        // Raw mode does not translate \n, so return to column 0 explicitly.
        write!(out, "{line}\r\n")?;
    }
    out.flush()
}
