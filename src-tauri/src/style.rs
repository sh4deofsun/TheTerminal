//! Cell → styled-run conversion with a fixed dark palette (v1: single theme).

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use serde::Serialize;

pub const DEFAULT_FG: &str = "#dcdfe4";
pub const DEFAULT_BG: &str = "#1e2227";

/// One Dark inspired 16-color base.
const BASE16: [&str; 16] = [
    "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#dcdfe4",
    "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
];

/// Style bitflags mirrored in the frontend: 1 bold, 2 italic, 4 underline, 8 dim, 16 strike.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Run {
    pub x: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub st: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

fn indexed(i: u8) -> String {
    if i < 16 {
        return BASE16[i as usize].to_string();
    }
    if i < 232 {
        // 6x6x6 cube
        let i = i - 16;
        let comp = |v: u8| if v == 0 { 0u8 } else { 55 + v * 40 };
        let r = comp(i / 36);
        let g = comp((i % 36) / 6);
        let b = comp(i % 6);
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        let v = 8 + (i - 232) * 10;
        format!("#{v:02x}{v:02x}{v:02x}")
    }
}

fn rgb(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// None = default fg/bg (frontend CSS decides).
fn resolve(c: Color) -> Option<String> {
    match c {
        Color::Named(n) => match n {
            NamedColor::Foreground
            | NamedColor::BrightForeground
            | NamedColor::Background
            | NamedColor::Cursor => None,
            // ponytail: Dim* mapped to base colors; dim rendered via flag opacity
            NamedColor::DimBlack => Some(BASE16[0].into()),
            NamedColor::DimRed => Some(BASE16[1].into()),
            NamedColor::DimGreen => Some(BASE16[2].into()),
            NamedColor::DimYellow => Some(BASE16[3].into()),
            NamedColor::DimBlue => Some(BASE16[4].into()),
            NamedColor::DimMagenta => Some(BASE16[5].into()),
            NamedColor::DimCyan => Some(BASE16[6].into()),
            NamedColor::DimWhite | NamedColor::DimForeground => Some(BASE16[7].into()),
            n => Some(BASE16[(n as usize).min(15)].into()),
        },
        Color::Spec(c) => Some(rgb(c)),
        Color::Indexed(i) => Some(indexed(i)),
    }
}

pub fn cells_to_runs<'a>(cells: impl Iterator<Item = &'a Cell>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for cell in cells {
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
            || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let mut fg = resolve(cell.fg);
        let mut bg = resolve(cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            let f = fg.take().unwrap_or_else(|| DEFAULT_FG.into());
            let b = bg.take().unwrap_or_else(|| DEFAULT_BG.into());
            fg = Some(b);
            bg = Some(f);
        }
        if cell.flags.contains(Flags::HIDDEN) {
            fg = bg.clone();
        }
        let mut st = 0u8;
        if cell.flags.contains(Flags::BOLD) {
            st |= 1;
        }
        if cell.flags.contains(Flags::ITALIC) {
            st |= 2;
        }
        if cell.flags.intersects(Flags::ALL_UNDERLINES) {
            st |= 4;
        }
        if cell.flags.contains(Flags::DIM) {
            st |= 8;
        }
        if cell.flags.contains(Flags::STRIKEOUT) {
            st |= 16;
        }
        let ch = if cell.c == '\0' { ' ' } else { cell.c };
        match runs.last_mut() {
            Some(r) if r.fg == fg && r.bg == bg && r.st == st => r.x.push(ch),
            _ => runs.push(Run {
                x: ch.to_string(),
                fg,
                bg,
                st,
            }),
        }
    }
    // Trim trailing unstyled whitespace.
    while let Some(last) = runs.last_mut() {
        if last.bg.is_none() && last.st & 16 == 0 && last.st & 4 == 0 {
            let trimmed = last.x.trim_end();
            if trimmed.is_empty() {
                runs.pop();
                continue;
            }
            if trimmed.len() != last.x.len() {
                last.x = trimmed.to_string();
            }
        }
        break;
    }
    runs
}

pub fn runs_empty(runs: &[Run]) -> bool {
    runs.iter().all(|r| r.x.trim().is_empty() && r.bg.is_none())
}
