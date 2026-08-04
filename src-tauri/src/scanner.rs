//! Incremental OSC 133 / OSC 7 interceptor.
//!
//! Splits a PTY byte stream into passthrough segments and intercepted marks.
//! Every non-intercepted byte is forwarded verbatim, in order, so the VT
//! parser downstream sees an untouched stream (minus our two OSC numbers).

const MAX_PAYLOAD: usize = 2048;

#[derive(Debug, PartialEq)]
pub enum Seg {
    /// Raw bytes to feed to the terminal.
    Out(Vec<u8>),
    /// Intercepted mark: (osc_number, payload) e.g. (133, "D;0") or (7, "file://host/path").
    Mark(u16, String),
}

#[derive(Default)]
enum St {
    #[default]
    Ground,
    Esc,
    OscNum,
    OscPayload,
    OscSt,
}

#[derive(Default)]
pub struct OscScanner {
    st: St,
    /// Bytes of a potential intercept sequence, held until resolved.
    pend: Vec<u8>,
    num: String,
    payload: Vec<u8>,
}

impl OscScanner {
    pub fn feed(&mut self, input: &[u8]) -> Vec<Seg> {
        let mut segs: Vec<Seg> = Vec::new();
        let mut out: Vec<u8> = Vec::new();

        macro_rules! flush_pend {
            () => {
                out.append(&mut self.pend);
            };
        }

        let mut i = 0;
        while i < input.len() {
            let b = input[i];
            let mut consumed = true;
            match self.st {
                St::Ground => {
                    if b == 0x1b {
                        self.pend.clear();
                        self.pend.push(b);
                        self.st = St::Esc;
                    } else {
                        out.push(b);
                    }
                }
                St::Esc => {
                    if b == b']' {
                        self.pend.push(b);
                        self.num.clear();
                        self.st = St::OscNum;
                    } else {
                        // Not an OSC — forward held ESC, reprocess byte in Ground.
                        flush_pend!();
                        self.st = St::Ground;
                        consumed = false;
                    }
                }
                St::OscNum => {
                    if b.is_ascii_digit() && self.num.len() < 5 {
                        self.pend.push(b);
                        self.num.push(b as char);
                    } else if b == b';' && (self.num == "133" || self.num == "7") {
                        self.pend.push(b);
                        self.payload.clear();
                        self.st = St::OscPayload;
                    } else {
                        // Some other OSC — forward everything, let the VT parser handle it.
                        flush_pend!();
                        self.st = St::Ground;
                        consumed = false;
                    }
                }
                St::OscPayload => {
                    if b == 0x07 {
                        self.emit_mark(&mut segs, &mut out);
                    } else if b == 0x1b {
                        self.st = St::OscSt;
                    } else if self.payload.len() >= MAX_PAYLOAD {
                        // Runaway payload — abort intercept, forward raw.
                        flush_pend!();
                        out.extend_from_slice(&self.payload);
                        self.payload.clear();
                        self.st = St::Ground;
                        consumed = false;
                    } else {
                        self.payload.push(b);
                    }
                }
                St::OscSt => {
                    if b == b'\\' {
                        self.emit_mark(&mut segs, &mut out);
                    } else {
                        // Lone ESC inside payload (malformed) — keep it, reprocess.
                        self.payload.push(0x1b);
                        self.st = St::OscPayload;
                        consumed = false;
                    }
                }
            }
            if consumed {
                i += 1;
            }
        }

        if !out.is_empty() {
            segs.push(Seg::Out(out));
        }
        segs
    }

    fn emit_mark(&mut self, segs: &mut Vec<Seg>, out: &mut Vec<u8>) {
        if !out.is_empty() {
            segs.push(Seg::Out(std::mem::take(out)));
        }
        let num: u16 = self.num.parse().unwrap_or(0);
        let payload = String::from_utf8_lossy(&self.payload).into_owned();
        segs.push(Seg::Mark(num, payload));
        self.pend.clear();
        self.payload.clear();
        self.st = St::Ground;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outs(segs: &[Seg]) -> Vec<u8> {
        let mut v = Vec::new();
        for s in segs {
            if let Seg::Out(b) = s {
                v.extend_from_slice(b);
            }
        }
        v
    }

    #[test]
    fn intercepts_133_bel() {
        let mut s = OscScanner::default();
        let segs = s.feed(b"hi\x1b]133;D;0\x07yo");
        assert_eq!(
            segs,
            vec![
                Seg::Out(b"hi".to_vec()),
                Seg::Mark(133, "D;0".into()),
                Seg::Out(b"yo".to_vec())
            ]
        );
    }

    #[test]
    fn intercepts_st_terminator() {
        let mut s = OscScanner::default();
        let segs = s.feed(b"\x1b]7;file://h/tmp\x1b\\x");
        assert_eq!(
            segs,
            vec![Seg::Mark(7, "file://h/tmp".into()), Seg::Out(b"x".to_vec())]
        );
    }

    #[test]
    fn passes_other_osc_verbatim() {
        let mut s = OscScanner::default();
        let segs = s.feed(b"\x1b]0;title\x07after");
        assert_eq!(outs(&segs), b"\x1b]0;title\x07after".to_vec());
        assert!(!segs.iter().any(|s| matches!(s, Seg::Mark(..))));
    }

    #[test]
    fn split_across_chunks() {
        let mut s = OscScanner::default();
        let mut segs = s.feed(b"a\x1b]13");
        segs.extend(s.feed(b"3;C"));
        segs.extend(s.feed(b"\x07b"));
        assert_eq!(
            segs,
            vec![
                Seg::Out(b"a".to_vec()),
                Seg::Mark(133, "C".into()),
                Seg::Out(b"b".to_vec())
            ]
        );
    }

    #[test]
    fn esc_not_osc_flushes() {
        let mut s = OscScanner::default();
        let segs = s.feed(b"\x1b[31mred");
        assert_eq!(outs(&segs), b"\x1b[31mred".to_vec());
    }

    #[test]
    fn csi_sgr_after_partial_esc_chunk() {
        let mut s = OscScanner::default();
        let mut segs = s.feed(b"x\x1b");
        segs.extend(s.feed(b"[0m"));
        assert_eq!(outs(&segs), b"x\x1b[0m".to_vec());
    }
}
