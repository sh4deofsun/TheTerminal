/** Fig-spec based completion source for CodeMirror (decision 9). */
import type { CompletionContext, CompletionResult, Completion } from "@codemirror/autocomplete";
import { completePath, listBins } from "./ipc";

type FigName = string | string[];
type FigArg = {
  name?: string;
  template?: string | string[];
  suggestions?: (string | { name: FigName; description?: string })[];
  isVariadic?: boolean;
};
type FigOption = { name: FigName; description?: string; args?: FigArg | FigArg[] };
type FigSubcommand = {
  name: FigName;
  description?: string;
  subcommands?: FigSubcommand[];
  options?: FigOption[];
  args?: FigArg | FigArg[];
};

const specCache = new Map<string, FigSubcommand | null>();
let binsCache: string[] | null = null;
let historyCache: string[] = [];

export function setHistory(h: string[]) {
  historyCache = h;
}
export function pushHistory(cmd: string) {
  historyCache = [cmd, ...historyCache.filter((c) => c !== cmd)];
}
export function getHistory(): string[] {
  return historyCache;
}

async function loadSpec(cmd: string): Promise<FigSubcommand | null> {
  if (specCache.has(cmd)) return specCache.get(cmd)!;
  if (!/^[\w@][\w@.+-]*$/.test(cmd)) {
    specCache.set(cmd, null);
    return null;
  }
  try {
    const mod = await import(/* @vite-ignore */ `/figspecs/${cmd}.js`);
    specCache.set(cmd, mod.default ?? null);
  } catch {
    specCache.set(cmd, null);
  }
  return specCache.get(cmd)!;
}

const names = (n: FigName): string[] => (Array.isArray(n) ? n : [n]);
const argsOf = (a?: FigArg | FigArg[]): FigArg[] =>
  a == null ? [] : Array.isArray(a) ? a : [a];

/** Crude shell tokenizer: whitespace-split, quote-aware, pipeline-aware (last segment). */
function tokenize(line: string): { toks: string[]; trailingSpace: boolean } {
  // Only complete the last pipeline segment.
  let seg = line;
  for (const sep of ["|", "&&", "||", ";"]) {
    const idx = seg.lastIndexOf(sep);
    if (idx >= 0) seg = seg.slice(idx + sep.length);
  }
  const toks: string[] = [];
  let cur = "";
  let q: string | null = null;
  for (const ch of seg) {
    if (q) {
      cur += ch;
      if (ch === q) q = null;
    } else if (ch === "'" || ch === '"') {
      q = ch;
      cur += ch;
    } else if (/\s/.test(ch)) {
      if (cur) toks.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  const trailingSpace = /\s$/.test(seg) || seg === "";
  if (cur) toks.push(cur);
  return { toks, trailingSpace };
}

function findSub(spec: FigSubcommand, tok: string): FigSubcommand | undefined {
  return spec.subcommands?.find((s) => names(s.name).includes(tok));
}

function hasFileTemplate(args: FigArg[]): { files: boolean; folders: boolean } {
  let files = false,
    folders = false;
  for (const a of args) {
    const t = a.template ? (Array.isArray(a.template) ? a.template : [a.template]) : [];
    if (t.includes("filepaths") || t.includes("history")) files = true;
    if (t.includes("folders")) folders = true;
  }
  return { files, folders };
}

export function makeCompletionSource(getSessionId: () => number | null) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const line = ctx.state.sliceDoc(0, ctx.pos);
    const { toks, trailingSpace } = tokenize(line);
    const curTok = trailingSpace ? "" : (toks[toks.length - 1] ?? "");
    const prevToks = trailingSpace ? toks : toks.slice(0, -1);
    const from = ctx.pos - curTok.length;

    if (!ctx.explicit && curTok.length === 0) return null;

    const options: Completion[] = [];
    const sid = getSessionId();

    if (prevToks.length === 0) {
      // Command position: binaries + history lines.
      if (!binsCache) binsCache = await listBins().catch(() => []);
      const seen = new Set<string>();
      for (const h of historyCache) {
        if (h.startsWith(line) && h !== line && seen.size < 8 && !seen.has(h)) {
          seen.add(h);
          options.push({
            label: h,
            type: "history",
            apply: h,
            boost: 2,
            detail: "history",
          });
        }
      }
      for (const b of binsCache) {
        if (b.startsWith(curTok)) {
          options.push({ label: b, type: "function", boost: 1 });
          if (options.length > 300) break;
        }
      }
      // Path completion when it looks like a path.
      if (/^[.~/]/.test(curTok) && sid != null) {
        await addPathOptions(sid, curTok, from, options, false);
        return pathResult(curTok, from, options);
      }
      return options.length ? { from, options, validFor: /^[\w./~@-]*$/ } : null;
    }

    // Argument position: walk the Fig spec.
    const cmd = prevToks[0].replace(/^.*\//, ""); // strip path prefix
    let spec = await loadSpec(cmd);
    let node: FigSubcommand | null = spec;
    if (node) {
      for (const t of prevToks.slice(1)) {
        if (t.startsWith("-")) continue;
        const sub = findSub(node, t);
        if (sub) node = sub;
      }
    }

    let wantFiles = true; // default: offer paths
    let wantFolders = false;

    if (node) {
      if (curTok.startsWith("-") || curTok === "") {
        for (const o of node.options ?? []) {
          for (const n of names(o.name)) {
            if (n.startsWith(curTok || "-")) {
              options.push({
                label: n,
                type: "keyword",
                detail: o.description?.slice(0, 80),
                boost: 1,
              });
            }
          }
        }
      }
      if (!curTok.startsWith("-")) {
        for (const s of node.subcommands ?? []) {
          const n = names(s.name)[0];
          if (n.startsWith(curTok)) {
            options.push({
              label: n,
              type: "class",
              detail: s.description?.slice(0, 80),
              boost: 3,
            });
          }
        }
        const args = argsOf(node.args);
        for (const a of args) {
          for (const sug of a.suggestions ?? []) {
            const n = typeof sug === "string" ? sug : names(sug.name)[0];
            if (n.startsWith(curTok)) options.push({ label: n, type: "constant" });
          }
        }
        const t = hasFileTemplate(args);
        wantFiles = t.files || args.length === 0;
        wantFolders = t.folders;
      }
    }

    if (!curTok.startsWith("-") && (wantFiles || wantFolders) && sid != null) {
      await addPathOptions(sid, curTok, from, options, wantFolders && !wantFiles);
    }

    return options.length
      ? { from: pathFrom(curTok, from), options: rebase(options, curTok, from), validFor: /^[^\s]*$/ }
      : null;
  };
}

/**
 * Paths complete only the last segment; other options complete the whole token.
 * CodeMirror needs one `from` — use token start and give path options an apply
 * that preserves the dir prefix.
 */
function pathFrom(_cur: string, from: number) {
  return from;
}
function rebase(options: Completion[], _cur: string, _from: number) {
  return options;
}

async function addPathOptions(
  sid: number,
  curTok: string,
  _from: number,
  options: Completion[],
  foldersOnly: boolean,
) {
  const entries = await completePath(sid, curTok).catch(() => []);
  const slash = curTok.lastIndexOf("/");
  const dirPrefix = slash >= 0 ? curTok.slice(0, slash + 1) : "";
  for (const e of entries) {
    if (foldersOnly && !e.dir) continue;
    const label = dirPrefix + e.name + (e.dir ? "/" : "");
    options.push({
      label,
      type: e.dir ? "namespace" : "text",
      boost: e.dir ? 1 : 0,
    });
  }
}

function pathResult(
  _curTok: string,
  from: number,
  options: Completion[],
): CompletionResult | null {
  return options.length ? { from, options, validFor: /^[^\s]*$/ } : null;
}
