/** KeyboardEvent → PTY byte string (raw mode input). Returns null to let the browser handle it. */
export function encodeKey(e: KeyboardEvent, appCursor: boolean): string | null {
  if (e.metaKey) return null; // cmd shortcuts belong to the app

  const ss3 = (c: string) => (appCursor ? `\x1bO${c}` : `\x1b[${c}`);

  switch (e.key) {
    case "Enter":
      return "\r";
    case "Backspace":
      return "\x7f";
    case "Tab":
      return e.shiftKey ? "\x1b[Z" : "\t";
    case "Escape":
      return "\x1b";
    case "ArrowUp":
      return ss3("A");
    case "ArrowDown":
      return ss3("B");
    case "ArrowRight":
      return ss3("C");
    case "ArrowLeft":
      return ss3("D");
    case "Home":
      return ss3("H");
    case "End":
      return ss3("F");
    case "PageUp":
      return "\x1b[5~";
    case "PageDown":
      return "\x1b[6~";
    case "Delete":
      return "\x1b[3~";
    case "Insert":
      return "\x1b[2~";
  }

  if (/^F(\d{1,2})$/.test(e.key)) {
    const n = parseInt(e.key.slice(1), 10);
    const f: Record<number, string> = {
      1: "\x1bOP", 2: "\x1bOQ", 3: "\x1bOR", 4: "\x1bOS",
      5: "\x1b[15~", 6: "\x1b[17~", 7: "\x1b[18~", 8: "\x1b[19~",
      9: "\x1b[20~", 10: "\x1b[21~", 11: "\x1b[23~", 12: "\x1b[24~",
    };
    return f[n] ?? null;
  }

  if (e.key.length !== 1) return null;

  if (e.ctrlKey) {
    const c = e.key.toLowerCase();
    if (c >= "a" && c <= "z")
      return String.fromCharCode(c.charCodeAt(0) - 96);
    const map: Record<string, string> = {
      " ": "\x00", "[": "\x1b", "\\": "\x1c", "]": "\x1d", "^": "\x1e", _: "\x1f",
    };
    return map[e.key] ?? null;
  }

  if (e.altKey) return "\x1b" + e.key;
  return e.key;
}
