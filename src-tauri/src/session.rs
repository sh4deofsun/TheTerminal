//! One terminal session: PTY + zsh + alacritty VT grid + OSC 133 block state machine.
//!
//! Absolute-line accounting: `abs_scrolled` counts lines pushed into scrollback
//! since session start. A block opening at grid row R gets `start_abs =
//! abs_scrolled + R`; every later row's block-relative index is
//! `abs + row - start_abs`, stable across scrolling.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config as TermConfig, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;

use crate::scanner::{OscScanner, Seg};
use crate::style::{cells_to_runs, runs_empty, Run};

pub type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Serialize, Clone)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Ev {
    /// Block content rows: (block-relative row index, styled runs).
    Rows { block: u64, rows: Vec<(usize, Vec<Run>)> },
    Open { block: u64 },
    Close {
        block: u64,
        exit: Option<i32>,
        dur_ms: u64,
        len: usize,
        truncated: bool,
    },
    Mode {
        mode: String, // "prompt" | "running" | "alt"
        cwd: String,
        app_cursor: bool,
        bracketed: bool,
    },
    Screen {
        rows: Vec<(usize, Vec<Run>)>,
        cursor: (usize, usize),
        cols: usize,
        lines: usize,
    },
    Cursor {
        block: u64,
        row: i64,
        col: usize,
        visible: bool,
    },
    Exited {},
}

struct SizeDim {
    cols: usize,
    lines: usize,
}

impl Dimensions for SizeDim {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Forwards terminal-generated responses (DA, DSR...) back to the PTY.
struct Proxy {
    writer: SharedWriter,
}

impl EventListener for Proxy {
    fn send_event(&self, ev: Event) {
        if let Event::PtyWrite(s) = ev {
            let _ = self.writer.lock().unwrap().write_all(s.as_bytes());
        }
    }
}

#[derive(PartialEq)]
enum ShellState {
    Running,
    Prompt,
}

struct BlockState {
    id: u64,
    start_abs: u64,
    started: Instant,
    max_rel: Option<usize>,
    truncated: bool,
}

pub struct Session {
    term: Term<Proxy>,
    processor: Processor,
    scanner: OscScanner,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    state: ShellState,
    cur_block: Option<BlockState>,
    next_block_id: u64,
    abs_scrolled: u64,
    prev_hist: usize,
    was_alt: bool,
    flags: (bool, bool), // app_cursor, bracketed
    cwd: String,
    scrollback: usize,
    force_full: bool,
    last_mode_sent: Option<(String, String, bool, bool)>,
    last_cursor: Option<(u64, i64, usize, bool)>,
    pub pending: Vec<Ev>,
    pub dead: bool,
}

impl Session {
    pub fn spawn(
        scrollback: usize,
        zdot: Option<&std::path::Path>,
    ) -> Result<(Self, SharedWriter, Box<dyn Read + Send>), String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;

        let home = dirs::home_dir().ok_or("no home dir")?;
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| s.ends_with("zsh"))
            .unwrap_or_else(|| "/bin/zsh".into());

        let mut cmd = CommandBuilder::new(&shell);
        cmd.arg("-l");
        cmd.cwd(&home); // decision 10: new sessions always start at $HOME
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "theterminal");
        if let Some(zdot) = zdot {
            let user_zdot = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.display().to_string());
            cmd.env("ZDOTDIR", zdot);
            cmd.env("TERMAPP_USER_ZDOTDIR", user_zdot);
        }

        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer: SharedWriter =
            Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));

        let term_cfg = TermConfig {
            scrolling_history: scrollback,
            ..Default::default()
        };
        let term = Term::new(
            term_cfg,
            &SizeDim { cols: 80, lines: 24 },
            Proxy {
                writer: writer.clone(),
            },
        );

        let now = Instant::now();
        let mut s = Session {
            term,
            processor: Processor::new(),
            scanner: OscScanner::default(),
            master: pair.master,
            child,
            state: ShellState::Running,
            // Implicit block 0 captures shell startup output; if OSC 133 never
            // arrives (hook off, exec'd shell) the session stays in this raw
            // single-block mode — decision 13's fallback.
            cur_block: Some(BlockState {
                id: 0,
                start_abs: 0,
                started: now,
                max_rel: None,
                truncated: false,
            }),
            next_block_id: 1,
            abs_scrolled: 0,
            prev_hist: 0,
            was_alt: false,
            flags: (false, false),
            cwd: home.display().to_string(),
            scrollback,
            force_full: false,
            last_mode_sent: None,
            last_cursor: None,
            pending: vec![Ev::Open { block: 0 }],
            dead: false,
        };
        s.push_mode();
        Ok((s, writer, reader))
    }

    pub fn feed(&mut self, data: &[u8]) {
        for seg in self.scanner.feed(data) {
            match seg {
                Seg::Out(bytes) => {
                    self.processor.advance(&mut self.term, &bytes);
                    self.after_advance();
                }
                Seg::Mark(133, payload) => self.mark_133(&payload),
                Seg::Mark(7, payload) => self.set_cwd(&payload),
                Seg::Mark(..) => {}
            }
        }
    }

    fn mark_133(&mut self, payload: &str) {
        let (kind, rest) = payload.split_at(1);
        match kind {
            "B" => {
                // Shell is idle at prompt. Close any still-open block (implicit
                // block 0, or a mark-less stretch) without an exit code.
                if self.cur_block.is_some() {
                    self.close_block(None);
                }
                self.state = ShellState::Prompt;
                self.push_mode();
            }
            "C" => {
                // preexec fired: command echo is already above the cursor, so
                // output starting at the cursor line excludes it (decision 8).
                if self.cur_block.is_some() {
                    self.close_block(None);
                }
                let row = self.term.grid().cursor.point.line.0.max(0) as u64;
                let id = self.next_block_id;
                self.next_block_id += 1;
                self.cur_block = Some(BlockState {
                    id,
                    start_abs: self.abs_scrolled + row,
                    started: Instant::now(),
                    max_rel: None,
                    truncated: false,
                });
                self.state = ShellState::Running;
                self.pending.push(Ev::Open { block: id });
                self.push_mode();
            }
            "D" => {
                let exit = rest.strip_prefix(';').and_then(|s| s.parse::<i32>().ok());
                self.close_block(exit);
            }
            _ => {} // "A" and unknown submarks: nothing to do
        }
    }

    fn set_cwd(&mut self, payload: &str) {
        // "file://host/path" → decode path part
        if let Some(idx) = payload.find("://") {
            let rest = &payload[idx + 3..];
            if let Some(slash) = rest.find('/') {
                let path = &rest[slash..];
                if path != self.cwd {
                    self.cwd = path.to_string();
                    self.push_mode();
                }
            }
        }
    }

    fn close_block(&mut self, exit: Option<i32>) {
        let Some(mut b) = self.cur_block.take() else {
            return;
        };
        // Harvest visible rows from block start through the cursor line.
        let grid_start = b.start_abs as i64 - self.abs_scrolled as i64;
        let cur_line = self.term.grid().cursor.point.line.0 as i64;
        let mut rows = Vec::new();
        for r in grid_start.max(0)..=cur_line {
            let rel = (self.abs_scrolled as i64 + r - b.start_abs as i64) as usize;
            let runs = self.line_runs(Line(r as i32));
            if !runs_empty(&runs) {
                b.max_rel = Some(b.max_rel.map_or(rel, |m| m.max(rel)));
            }
            rows.push((rel, runs));
        }
        if !rows.is_empty() {
            self.pending.push(Ev::Rows {
                block: b.id,
                rows,
            });
        }
        self.pending.push(Ev::Close {
            block: b.id,
            exit,
            dur_ms: b.started.elapsed().as_millis() as u64,
            len: b.max_rel.map_or(0, |m| m + 1),
            truncated: b.truncated,
        });
        self.last_cursor = None;
    }

    fn after_advance(&mut self) {
        // Alt-screen toggle: grids swap, history accounting must reset.
        let alt = self.term.mode().contains(TermMode::ALT_SCREEN);
        if alt != self.was_alt {
            self.was_alt = alt;
            self.prev_hist = self.term.grid().history_size();
            self.force_full = true;
            self.push_mode();
        }

        // Ship lines newly scrolled into history to the open block.
        let hist = self.term.grid().history_size();
        if hist > self.prev_hist && !alt {
            let n = hist - self.prev_hist;
            if let Some(b) = &self.cur_block {
                let (id, start_abs) = (b.id, b.start_abs);
                let mut rows = Vec::new();
                let mut new_max: Option<usize> = None;
                for k in 0..n {
                    let abs = self.abs_scrolled + k as u64;
                    if abs >= start_abs {
                        let rel = (abs - start_abs) as usize;
                        let runs = self.line_runs(Line(-((n - k) as i32)));
                        if !runs_empty(&runs) {
                            new_max = Some(new_max.map_or(rel, |m| m.max(rel)));
                        }
                        rows.push((rel, runs));
                    }
                }
                if let (Some(b), Some(m)) = (&mut self.cur_block, new_max) {
                    b.max_rel = Some(b.max_rel.map_or(m, |o| o.max(m)));
                }
                if !rows.is_empty() {
                    self.pending.push(Ev::Rows { block: id, rows });
                }
            }
            self.abs_scrolled += n as u64;
        }
        self.prev_hist = hist;

        // ponytail: at scrollback saturation line accounting degrades; flag the
        // block truncated instead of tracking scroll events (upgrade path:
        // custom Handler wrapper counting linefeeds).
        if hist >= self.scrollback {
            if let Some(b) = &mut self.cur_block {
                b.truncated = true;
            }
        }

        let mode = self.term.mode();
        let f = (
            mode.contains(TermMode::APP_CURSOR),
            mode.contains(TermMode::BRACKETED_PASTE),
        );
        if f != self.flags {
            self.flags = f;
            self.push_mode();
        }
    }

    fn push_mode(&mut self) {
        let mode = if self.was_alt {
            "alt"
        } else if self.state == ShellState::Prompt {
            "prompt"
        } else {
            "running"
        };
        let key = (
            mode.to_string(),
            self.cwd.clone(),
            self.flags.0,
            self.flags.1,
        );
        if self.last_mode_sent.as_ref() == Some(&key) {
            return;
        }
        self.last_mode_sent = Some(key);
        self.pending.push(Ev::Mode {
            mode: mode.into(),
            cwd: self.cwd.clone(),
            app_cursor: self.flags.0,
            bracketed: self.flags.1,
        });
    }

    fn line_runs(&self, l: Line) -> Vec<Run> {
        let grid = self.term.grid();
        let row = &grid[l];
        cells_to_runs((0..grid.columns()).map(|c| &row[Column(c)]))
    }

    /// Drain queued events + coalesced damage (called ~60fps by the ticker).
    pub fn flush(&mut self) -> Vec<Ev> {
        let mut evs = std::mem::take(&mut self.pending);
        if self.dead {
            return evs;
        }

        let screen_lines = self.term.screen_lines();
        let mut dirty: Vec<usize> = if self.force_full {
            (0..screen_lines).collect()
        } else {
            match self.term.damage() {
                TermDamage::Full => (0..screen_lines).collect(),
                TermDamage::Partial(it) => it.map(|d| d.line).collect(),
            }
        };
        self.force_full = false;
        self.term.reset_damage();
        dirty.retain(|&r| r < screen_lines);

        if self.was_alt {
            if !dirty.is_empty() {
                let rows = dirty
                    .iter()
                    .map(|&r| (r, self.line_runs(Line(r as i32))))
                    .collect();
                let p = self.term.grid().cursor.point;
                evs.push(Ev::Screen {
                    rows,
                    cursor: (p.line.0.max(0) as usize, p.column.0),
                    cols: self.term.columns(),
                    lines: screen_lines,
                });
            }
        } else if let Some(b) = &self.cur_block {
            let (id, start_abs) = (b.id, b.start_abs);
            let mut rows = Vec::new();
            let mut new_max: Option<usize> = None;
            for &r in &dirty {
                let abs = self.abs_scrolled as i64 + r as i64;
                if abs >= start_abs as i64 {
                    let rel = (abs - start_abs as i64) as usize;
                    let runs = self.line_runs(Line(r as i32));
                    if !runs_empty(&runs) {
                        new_max = Some(new_max.map_or(rel, |m| m.max(rel)));
                    }
                    rows.push((rel, runs));
                }
            }
            if let (Some(b), Some(m)) = (&mut self.cur_block, new_max) {
                b.max_rel = Some(b.max_rel.map_or(m, |o| o.max(m)));
            }
            if !rows.is_empty() {
                evs.push(Ev::Rows { block: id, rows });
            }
            // Live cursor within the running block.
            let p = self.term.grid().cursor.point;
            let visible = self.term.mode().contains(TermMode::SHOW_CURSOR)
                && self.state == ShellState::Running;
            let crow = self.abs_scrolled as i64 + p.line.0 as i64 - start_abs as i64;
            let cur = (id, crow, p.column.0, visible);
            if self.last_cursor != Some(cur) {
                self.last_cursor = Some(cur);
                evs.push(Ev::Cursor {
                    block: id,
                    row: crow,
                    col: p.column.0,
                    visible,
                });
            }
        }
        evs
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.term.resize(SizeDim {
            cols: cols as usize,
            lines: rows as usize,
        });
        self.prev_hist = self.term.grid().history_size();
        self.force_full = true;
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// End-to-end: real zsh + ZDOTDIR hook + scanner + block machine.
    /// Spawns the user's shell, runs `echo`, expects OSC 133 marks to produce
    /// a prompt mode, an opened block, output rows, and a clean close.
    #[test]
    fn e2e_echo_block() {
        let zdot = crate::zdot::setup().expect("zdot setup");
        let (mut s, writer, mut reader) =
            Session::spawn(1000, Some(&zdot)).expect("spawn zsh");

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut submitted = false;
        let mut got_open_gt0 = false;
        let mut got_output = false;
        let mut got_close = false;

        while Instant::now() < deadline && !got_close {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(bytes) => s.feed(&bytes),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            if !submitted && s.state == ShellState::Prompt {
                submitted = true;
                writer
                    .lock()
                    .unwrap()
                    .write_all(b"echo TT_E2E_42\r")
                    .unwrap();
            }
            for ev in &s.pending {
                match ev {
                    Ev::Open { block } if *block > 0 => got_open_gt0 = true,
                    Ev::Rows { block, rows } if *block > 0 => {
                        for (_, runs) in rows {
                            let text: String = runs.iter().map(|r| r.x.as_str()).collect();
                            if text.contains("TT_E2E_42") {
                                got_output = true;
                            }
                        }
                    }
                    Ev::Close { block, exit, .. } if *block > 0 => {
                        assert_eq!(*exit, Some(0), "echo must exit 0");
                        got_close = true;
                    }
                    _ => {}
                }
            }
            // Also fold in live-tail damage rows.
            for ev in s.flush() {
                if let Ev::Rows { block, rows } = ev {
                    if block > 0 {
                        for (_, runs) in &rows {
                            let text: String = runs.iter().map(|r| r.x.as_str()).collect();
                            if text.contains("TT_E2E_42") {
                                got_output = true;
                            }
                        }
                    }
                }
            }
        }

        s.kill();
        assert!(submitted, "never reached prompt mode (OSC 133 B missing)");
        assert!(got_open_gt0, "no block opened (OSC 133 C missing)");
        assert!(got_output, "block output missing echo text");
        assert!(got_close, "block never closed (OSC 133 D missing)");
    }
}
