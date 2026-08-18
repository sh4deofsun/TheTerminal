mod alias;
mod config;
mod histfile;
mod scanner;
mod session;
mod style;
mod zdot;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use session::{Ev, Session, SharedWriter};

struct AppState {
    sessions: Mutex<HashMap<u64, Arc<Mutex<Session>>>>,
    writers: Mutex<HashMap<u64, SharedWriter>>,
    next_id: AtomicU64,
    cfg: config::Config,
    zdot: Option<PathBuf>,
}

#[derive(Serialize, Clone)]
struct SessEvt {
    id: u64,
    evs: Vec<Ev>,
}

#[tauri::command]
fn get_config(state: State<AppState>) -> config::Config {
    state.cfg.clone()
}

#[tauri::command]
fn create_session(state: State<AppState>) -> Result<u64, String> {
    let (sess, writer, mut reader) = Session::spawn(state.cfg.scrollback, state.zdot.as_deref())?;
    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    let sess = Arc::new(Mutex::new(sess));
    state.sessions.lock().unwrap().insert(id, sess.clone());
    state.writers.lock().unwrap().insert(id, writer);

    std::thread::spawn(move || {
        let mut buf = [0u8; 65536];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sess.lock().unwrap().feed(&buf[..n]),
            }
        }
        let mut s = sess.lock().unwrap();
        s.dead = true;
        s.pending.push(Ev::Exited {});
    });
    Ok(id)
}

#[tauri::command]
fn close_session(state: State<AppState>, id: u64) {
    state.writers.lock().unwrap().remove(&id);
    if let Some(sess) = state.sessions.lock().unwrap().remove(&id) {
        sess.lock().unwrap().kill();
    }
}

#[tauri::command]
fn write_session(state: State<AppState>, id: u64, data: String) -> Result<(), String> {
    let writer = state
        .writers
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or("no session")?;
    let r = writer
        .lock()
        .unwrap()
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string());
    r
}

#[tauri::command]
fn submit_command(state: State<AppState>, id: u64, text: String) -> Result<(), String> {
    let writer = state
        .writers
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or("no session")?;
    let payload = if text.contains('\n') {
        // Bracketed paste keeps zle from executing intermediate lines.
        format!("\x1b[200~{text}\x1b[201~\r")
    } else {
        format!("{text}\r")
    };
    let r = writer
        .lock()
        .unwrap()
        .write_all(payload.as_bytes())
        .map_err(|e| e.to_string());
    r
}

#[tauri::command]
fn resize_session(state: State<AppState>, id: u64, cols: u16, rows: u16) {
    if let Some(sess) = state.sessions.lock().unwrap().get(&id).cloned() {
        sess.lock().unwrap().resize(cols.max(2), rows.max(2));
    }
}

#[tauri::command]
fn list_aliases(state: State<AppState>) -> Vec<alias::Alias> {
    alias::list(state.zdot.as_deref())
}

/// Saved aliases apply to sessions started afterwards.
#[tauri::command]
fn set_alias(name: String, value: String) -> Result<(), String> {
    alias::set(&name, &value)
}

/// Ours is dropped; an rc-defined one is suppressed with `unalias`.
#[tauri::command]
fn remove_alias(name: String) -> Result<(), String> {
    alias::remove(&name)
}

#[tauri::command]
fn read_history() -> Vec<String> {
    histfile::read_history(3000)
}

#[derive(Serialize)]
struct PathEntry {
    name: String,
    dir: bool,
}

#[tauri::command]
fn complete_path(state: State<AppState>, id: u64, prefix: String) -> Vec<PathEntry> {
    let cwd = state
        .sessions
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(|s| s.lock().unwrap().cwd().to_string())
        .unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let expanded = if let Some(rest) = prefix.strip_prefix("~/") {
        home.join(rest)
    } else if prefix == "~" {
        home.clone()
    } else if prefix.starts_with('/') {
        PathBuf::from(&prefix)
    } else {
        PathBuf::from(&cwd).join(&prefix)
    };

    let (dir, base) = if prefix.ends_with('/') || prefix.is_empty() || prefix == "~" {
        (expanded, String::new())
    } else {
        (
            expanded
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| expanded.clone()),
            expanded
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };

    let base_l = base.to_lowercase();
    let mut out: Vec<PathEntry> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.to_lowercase().starts_with(&base_l) {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                out.push(PathEntry { name, dir: is_dir });
                if out.len() >= 200 {
                    break;
                }
            }
        }
    }
    out.sort_by_key(|e| (!e.dir, e.name.to_lowercase()));
    out
}

static BINS: OnceLock<Vec<String>> = OnceLock::new();

#[tauri::command]
fn list_bins() -> Vec<String> {
    BINS.get_or_init(|| {
        let mut set = std::collections::BTreeSet::new();
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for e in rd.flatten() {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            if e.metadata()
                                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                                .unwrap_or(false)
                            {
                                set.insert(e.file_name().to_string_lossy().into_owned());
                            }
                        }
                    }
                }
            }
        }
        set.into_iter().collect()
    })
    .clone()
}

fn start_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(16));
        let sessions: Vec<(u64, Arc<Mutex<Session>>)> = {
            let state = app.state::<AppState>();
            let map = state.sessions.lock().unwrap();
            map.iter().map(|(k, v)| (*k, v.clone())).collect()
        };
        for (id, sess) in sessions {
            let evs = sess.lock().unwrap().flush();
            if !evs.is_empty() {
                let _ = app.emit("sess", SessEvt { id, evs });
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = config::load();
    let zdot = if cfg.hook { zdot::setup().ok() } else { None };
    let state = AppState {
        sessions: Mutex::new(HashMap::new()),
        writers: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        cfg,
        zdot,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            start_ticker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            create_session,
            close_session,
            write_session,
            submit_command,
            resize_session,
            read_history,
            complete_path,
            list_bins,
            list_aliases,
            set_alias,
            remove_alias
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
