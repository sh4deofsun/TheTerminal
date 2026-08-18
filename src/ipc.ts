import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Run = { x: string; fg?: string; bg?: string; st?: number };
export type RowUpdate = [number, Run[]];

export type Ev =
  | { t: "rows"; block: number; rows: RowUpdate[] }
  | { t: "open"; block: number }
  | {
      t: "close";
      block: number;
      exit: number | null;
      dur_ms: number;
      len: number;
      truncated: boolean;
    }
  | { t: "mode"; mode: "prompt" | "running" | "alt"; cwd: string; app_cursor: boolean; bracketed: boolean }
  | { t: "screen"; rows: RowUpdate[]; cursor: [number, number]; cols: number; lines: number }
  | { t: "cursor"; block: number; row: number; col: number; visible: boolean }
  | { t: "exited" };

export type SessEvt = { id: number; evs: Ev[] };

export type AppConfig = {
  font_family: string;
  font_size: number;
  scrollback: number;
  autocomplete: boolean;
  hook: boolean;
};

export type PathEntry = { name: string; dir: boolean };

export type Alias = { name: string; value: string; own: boolean };

export const getConfig = () => invoke<AppConfig>("get_config");
export const createSession = () => invoke<number>("create_session");
export const closeSession = (id: number) => invoke("close_session", { id });
export const writeSession = (id: number, data: string) => invoke("write_session", { id, data });
export const submitCommand = (id: number, text: string) => invoke("submit_command", { id, text });
export const resizeSession = (id: number, cols: number, rows: number) =>
  invoke("resize_session", { id, cols, rows });
export const readHistory = () => invoke<string[]>("read_history");
export const completePath = (id: number, prefix: string) =>
  invoke<PathEntry[]>("complete_path", { id, prefix });
export const listBins = () => invoke<string[]>("list_bins");
export const listAliases = () => invoke<Alias[]>("list_aliases");
export const setAlias = (name: string, value: string) =>
  invoke("set_alias", { name, value });
export const removeAlias = (name: string) => invoke("remove_alias", { name });

export const onSessEvt = (cb: (e: SessEvt) => void) =>
  listen<SessEvt>("sess", (e) => cb(e.payload));
