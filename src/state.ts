import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { Ev, Run } from "./ipc";

export type Block = {
  id: number;
  cmd?: string;
  lines: (Run[] | undefined)[];
  running: boolean;
  exit?: number | null;
  durMs?: number;
  truncated?: boolean;
  expanded?: boolean;
};

export type Mode = "prompt" | "running" | "alt";

export type Screen = {
  rows: (Run[] | undefined)[];
  cursor: [number, number];
  cols: number;
  lines: number;
};

export type Session = {
  id: number;
  name: string;
  cwd: string;
  mode: Mode;
  blocks: Block[];
  screen: Screen;
  appCursor: boolean;
  bracketed: boolean;
  exited: boolean;
  cursor?: { block: number; row: number; col: number; visible: boolean };
  pendingCmd?: string;
  lastExit?: number | null;
};

/// While a modal is open the terminal must not swallow keystrokes.
export const [modalOpen, setModalOpen] = createSignal(false);

export const [store, setStore] = createStore<{
  sessions: Session[];
  active: number | null;
}>({ sessions: [], active: null });

let nameCounter = 0;

export function addSession(id: number) {
  nameCounter++;
  setStore(
    produce((s) => {
      s.sessions.push({
        id,
        name: `zsh ${nameCounter}`,
        cwd: "~",
        mode: "running",
        blocks: [],
        screen: { rows: [], cursor: [0, 0], cols: 80, lines: 24 },
        appCursor: false,
        bracketed: false,
        exited: false,
      });
      s.active = id;
    }),
  );
}

export function removeSession(id: number) {
  setStore(
    produce((s) => {
      const i = s.sessions.findIndex((x) => x.id === id);
      if (i < 0) return;
      s.sessions.splice(i, 1);
      if (s.active === id) {
        s.active = s.sessions[Math.min(i, s.sessions.length - 1)]?.id ?? null;
      }
    }),
  );
}

export const activeSession = () =>
  store.sessions.find((s) => s.id === store.active) ?? null;

export function applyEvents(id: number, evs: Ev[]) {
  setStore(
    produce((st) => {
      const s = st.sessions.find((x) => x.id === id);
      if (!s) return;
      for (const ev of evs) {
        switch (ev.t) {
          case "open": {
            s.blocks.push({
              id: ev.block,
              cmd: s.pendingCmd,
              lines: [],
              running: true,
            });
            s.pendingCmd = undefined;
            break;
          }
          case "rows": {
            let b = s.blocks.find((x) => x.id === ev.block);
            if (!b) break;
            for (const [i, runs] of ev.rows) b.lines[i] = runs;
            break;
          }
          case "close": {
            const b = s.blocks.find((x) => x.id === ev.block);
            if (!b) break;
            b.running = false;
            b.exit = ev.exit;
            b.durMs = ev.dur_ms;
            b.truncated = ev.truncated;
            b.lines.length = Math.min(b.lines.length, ev.len);
            if (ev.exit != null) s.lastExit = ev.exit;
            if (s.cursor?.block === ev.block) s.cursor = undefined;
            // Drop empty no-command blocks (e.g. silent startup).
            if (!b.cmd && b.lines.every((l) => !l || l.length === 0)) {
              s.blocks.splice(s.blocks.indexOf(b), 1);
            }
            break;
          }
          case "mode": {
            s.mode = ev.mode;
            s.cwd = ev.cwd;
            s.appCursor = ev.app_cursor;
            s.bracketed = ev.bracketed;
            break;
          }
          case "screen": {
            if (s.screen.lines !== ev.lines || s.screen.cols !== ev.cols) {
              s.screen.rows = new Array(ev.lines);
              s.screen.lines = ev.lines;
              s.screen.cols = ev.cols;
            }
            for (const [i, runs] of ev.rows) s.screen.rows[i] = runs;
            s.screen.cursor = ev.cursor;
            break;
          }
          case "cursor": {
            s.cursor = {
              block: ev.block,
              row: ev.row,
              col: ev.col,
              visible: ev.visible,
            };
            break;
          }
          case "exited": {
            s.exited = true;
            s.mode = "prompt";
            for (const b of s.blocks) b.running = false;
            break;
          }
        }
      }
    }),
  );
}

export function setPendingCmd(id: number, cmd: string) {
  setStore(
    produce((st) => {
      const s = st.sessions.find((x) => x.id === id);
      if (s) s.pendingCmd = cmd;
    }),
  );
}

export function renameSession(id: number, name: string) {
  setStore(
    produce((st) => {
      const s = st.sessions.find((x) => x.id === id);
      if (s && name.trim()) s.name = name.trim();
    }),
  );
}

export function toggleExpand(sid: number, bid: number) {
  setStore(
    produce((st) => {
      const b = st.sessions.find((x) => x.id === sid)?.blocks.find((x) => x.id === bid);
      if (b) b.expanded = !b.expanded;
    }),
  );
}

export function deleteBlock(sid: number, bid: number) {
  setStore(
    produce((st) => {
      const s = st.sessions.find((x) => x.id === sid);
      if (!s) return;
      const i = s.blocks.findIndex((x) => x.id === bid);
      if (i >= 0 && !s.blocks[i].running) s.blocks.splice(i, 1);
    }),
  );
}

export function clearBlocks(sid: number) {
  setStore(
    produce((st) => {
      const s = st.sessions.find((x) => x.id === sid);
      if (s) s.blocks = s.blocks.filter((b) => b.running);
    }),
  );
}
