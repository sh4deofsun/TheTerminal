//! zsh HISTFILE reader (decision 11: reuse the shell's own history).

/// zsh "metafies" bytes >= 0x83: 0x83 marker, next byte XOR 0x20.
fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x83 && i + 1 < bytes.len() {
            out.push(bytes[i + 1] ^ 0x20);
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Recent-first, deduped command list.
pub fn read_history(cap: usize) -> Vec<String> {
    let path = std::env::var("HISTFILE").map(Into::into).unwrap_or_else(|_| {
        dirs::home_dir()
            .unwrap_or_default()
            .join(".zsh_history")
    });
    let Ok(raw) = std::fs::read(&path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&unmetafy(&raw)).into_owned();

    let mut cmds: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if !cur.is_empty() {
            // Continuation of a multiline entry (previous line ended with backslash).
            cur.push('\n');
            cur.push_str(line);
        } else if let Some(rest) = line.strip_prefix(": ") {
            // Extended format: ": <ts>:<dur>;<cmd>"
            cur = rest.split_once(';').map(|(_, c)| c).unwrap_or(rest).to_string();
        } else if !line.is_empty() {
            cur = line.to_string();
        }
        if cur.ends_with('\\') {
            cur.pop();
            continue; // multiline entry continues
        }
        if !cur.trim().is_empty() {
            cmds.push(std::mem::take(&mut cur));
        } else {
            cur.clear();
        }
    }

    // Recent first, dedupe keeping most recent occurrence.
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in cmds.into_iter().rev() {
        if seen.insert(c.clone()) {
            out.push(c);
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmetafy_roundtrip() {
        // 0x83 0xa3 -> 0x83^0x20... payload byte 0xa3 ^ 0x20 = 0x83
        assert_eq!(unmetafy(&[b'a', 0x83, 0xa3, b'b']), vec![b'a', 0x83, b'b']);
        assert_eq!(unmetafy(b"plain"), b"plain".to_vec());
    }
}
