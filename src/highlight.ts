/** Tiny shell highlighter for block headers (decision 8: highlighted command snapshot). */
export type Tok = { text: string; cls: string };

export function highlightCmd(src: string): Tok[] {
  const toks: Tok[] = [];
  let i = 0;
  let cmdPos = true; // next bare word is a command name

  const push = (text: string, cls: string) => {
    if (text) toks.push({ text, cls });
  };

  while (i < src.length) {
    const c = src[i];
    if (c === "'" || c === '"') {
      let j = i + 1;
      while (j < src.length && src[j] !== c) {
        if (c === '"' && src[j] === "\\") j++;
        j++;
      }
      push(src.slice(i, Math.min(j + 1, src.length)), "tok-str");
      i = j + 1;
      cmdPos = false;
      continue;
    }
    if (/\s/.test(c)) {
      let j = i;
      while (j < src.length && /\s/.test(src[j])) j++;
      push(src.slice(i, j), "");
      i = j;
      continue;
    }
    if (c === "|" || c === "&" || c === ";") {
      let j = i;
      while (j < src.length && /[|&;]/.test(src[j])) j++;
      push(src.slice(i, j), "tok-op");
      i = j;
      cmdPos = true;
      continue;
    }
    // word
    let j = i;
    while (j < src.length && !/[\s|&;'"]/.test(src[j])) j++;
    const w = src.slice(i, j);
    if (cmdPos) {
      push(w, "tok-cmd");
      cmdPos = false;
    } else if (w.startsWith("-")) {
      push(w, "tok-flag");
    } else if (w.startsWith("$")) {
      push(w, "tok-var");
    } else if (w.includes("/") || w.startsWith("~")) {
      push(w, "tok-path");
    } else {
      push(w, "");
    }
    i = j;
  }
  return toks;
}
