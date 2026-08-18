//! Shell aliases: read the effective set from a real zsh, write overrides into
//! our own file. The user's rc files are never modified — our file is sourced
//! last by the ZDOTDIR wrapper, so an edited alias wins over the rc definition.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Alias {
    pub name: String,
    pub value: String,
    /// Managed by us (present in aliases.zsh) rather than coming from the rc files.
    pub own: bool,
}

pub fn file() -> Option<PathBuf> {
    crate::config::dir().map(|d| d.join("aliases.zsh"))
}

/// zsh single-quoted string: the only escape is `'` → `'\''`.
fn quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', r"'\''"))
}

fn unquote(v: &str) -> String {
    match v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        Some(inner) => inner.replace(r"'\''", "'"),
        None => v.to_string(),
    }
}

/// Names are written into a sourced shell file, so keep them to characters that
/// cannot break out of the `alias <name>=` position. A leading `-` would be read
/// as a flag by `alias`/`unalias`, so it is rejected too.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.:+".contains(c))
}

/// Our own file's entries. `None` means the alias is suppressed (`unalias`),
/// which is how an rc-defined alias gets removed without touching the rc file.
fn own_map() -> BTreeMap<String, Option<String>> {
    let mut out = BTreeMap::new();
    let Some(path) = file() else { return out };
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        if let Some((name, value)) = parse_line(line) {
            out.insert(name, Some(value));
        } else if let Some(name) = parse_unalias(line) {
            out.insert(name, None);
        }
    }
    out
}

/// `alias ll='ls -la'` → ("ll", "ls -la"). Ignores `alias -g`/`-s` and junk.
fn parse_line(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("alias ")?;
    if rest.starts_with('-') {
        return None; // global/suffix aliases: shown by zsh, not editable here
    }
    let (name, value) = rest.split_once('=')?;
    valid_name(name).then(|| (name.to_string(), unquote(value)))
}

fn parse_unalias(line: &str) -> Option<String> {
    let name = line
        .trim()
        .strip_prefix("unalias ")?
        .split_whitespace()
        .next()?;
    valid_name(name).then(|| name.to_string())
}

fn write_own(all: &BTreeMap<String, Option<String>>) -> Result<(), String> {
    let path = file().ok_or("config dizini bulunamadı")?;
    let mut text = String::from("# TheTerminal tarafından yönetilir — elle düzenlemeyin.\n");
    // Suppressions first: an rc alias must be gone before ours is defined.
    for (n, v) in all {
        if v.is_none() {
            text.push_str(&format!("unalias {n} 2>/dev/null\n"));
        }
    }
    for (n, v) in all {
        if let Some(v) = v {
            text.push_str(&format!("alias {n}={}\n", quote(v)));
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Effective aliases, asked of a real zsh so rc files, plugins and our override
/// file are all accounted for. `-l -i` mirrors how sessions are spawned: without
/// `-l` zsh skips `.zprofile`/`.zlogin`, so aliases defined there would be missed.
pub fn list(zdot: Option<&Path>) -> Vec<Alias> {
    let mut cmd = Command::new(crate::session::shell_path());
    cmd.args(["-l", "-i", "-c", "alias -L"]);
    if let Some(zdot) = zdot {
        let home = dirs::home_dir().unwrap_or_default();
        let user_zdot = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.display().to_string());
        cmd.env("ZDOTDIR", zdot);
        cmd.env("TERMAPP_USER_ZDOTDIR", user_zdot);
    }
    let Ok(out) = cmd.output() else {
        return Vec::new();
    };

    let own = own_map();
    let mut seen: BTreeMap<String, Alias> = BTreeMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some((name, value)) = parse_line(line) {
            let own = matches!(own.get(&name), Some(Some(_)));
            seen.insert(name.clone(), Alias { name, value, own });
        }
    }
    seen.into_values().collect()
}

/// Upsert one alias into our file.
pub fn set(name: &str, value: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("geçersiz alias adı: {name}"));
    }
    if value.contains('\n') {
        return Err("alias değeri tek satır olmalı".into());
    }
    let mut all = own_map();
    all.insert(name.to_string(), Some(value.to_string()));
    write_own(&all)
}

/// Drop one of ours, or suppress an rc-defined one with `unalias`.
pub fn remove(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("geçersiz alias adı: {name}"));
    }
    let mut all = own_map();
    all.insert(name.to_string(), None);
    write_own(&all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_roundtrip() {
        for v in ["ls -la", "echo 'hi'", "git commit -m 'a b'", "x''y"] {
            assert_eq!(unquote(&quote(v)), v, "roundtrip {v}");
        }
        // A quoted value can never terminate the alias line early.
        assert_eq!(
            parse_line(&format!("alias x={}", quote("a'; rm -rf /; '"))),
            Some(("x".into(), "a'; rm -rf /; '".into()))
        );
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(valid_name("ll"));
        assert!(valid_name("g.st"));
        assert!(!valid_name("a b"));
        assert!(!valid_name("a'b"));
        assert!(!valid_name("a=b"));
        assert!(!valid_name(""));
        assert!(!valid_name("-g")); // would be read as an `unalias`/`alias` flag
    }

    /// A removed alias must survive the file round-trip as a suppression, and a
    /// suppressed name must go back to being a normal alias when re-added.
    #[test]
    fn unalias_roundtrip() {
        assert_eq!(parse_unalias("unalias ll 2>/dev/null"), Some("ll".into()));
        assert_eq!(parse_unalias("unalias -m 'x'"), None);
        assert_eq!(parse_unalias("alias ll='x'"), None);

        let mut all = BTreeMap::new();
        all.insert("ll".to_string(), None);
        all.insert("gs".to_string(), Some("git status".to_string()));
        let mut text = String::new();
        for (n, v) in &all {
            match v {
                None => text.push_str(&format!("unalias {n} 2>/dev/null\n")),
                Some(v) => text.push_str(&format!("alias {n}={}\n", quote(v))),
            }
        }
        let mut back: BTreeMap<String, Option<String>> = BTreeMap::new();
        for line in text.lines() {
            if let Some((n, v)) = parse_line(line) {
                back.insert(n, Some(v));
            } else if let Some(n) = parse_unalias(line) {
                back.insert(n, None);
            }
        }
        assert_eq!(back, all);
    }

    #[test]
    fn skips_global_and_suffix_aliases() {
        assert_eq!(parse_line("alias -g L='| less'"), None);
        assert_eq!(parse_line("alias -s txt=vim"), None);
        assert_eq!(
            parse_line("alias ll='ls -la'"),
            Some(("ll".into(), "ls -la".into()))
        );
        // `alias -L` leaves values unquoted when they need no quoting.
        assert_eq!(
            parse_line("alias python=python3"),
            Some(("python".into(), "python3".into()))
        );
    }

    /// The real shell must be asked as a login shell, otherwise `.zprofile`
    /// and `.zlogin` aliases never reach the settings list.
    #[test]
    fn login_shell_sees_zprofile_aliases() {
        let dir = std::env::temp_dir().join("tt-alias-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".zprofile"), "alias ttprof='echo hi'\n").unwrap();
        std::fs::write(dir.join(".zshrc"), "").unwrap();
        std::fs::write(dir.join(".zshenv"), "").unwrap();

        let out = Command::new(crate::session::shell_path())
            .args(["-l", "-i", "-c", "alias -L"])
            .env("ZDOTDIR", &dir)
            .output()
            .unwrap();
        let found = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(parse_line)
            .any(|(n, v)| n == "ttprof" && v == "echo hi");
        std::fs::remove_dir_all(&dir).ok();
        assert!(found, "login shell must source .zprofile");
    }
}
