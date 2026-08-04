import { For, Show, createMemo } from "solid-js";
import type { Block, Session } from "../state";
import { toggleExpand, deleteBlock } from "../state";
import { highlightCmd } from "../highlight";
import { LineRuns, lineText } from "./Runs";
import { submitCommand } from "../ipc";
import { setPendingCmd } from "../state";

const COLLAPSE_AT = 400;
const TAIL_KEEP = 200;

function fmtDur(ms?: number): string {
  if (ms == null) return "";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60000)}m${Math.round((ms % 60000) / 1000)}s`;
}

export function BlockView(props: { block: Block; session: Session }) {
  const b = () => props.block;
  const failed = () => b().exit != null && b().exit !== 0;

  const visibleLines = createMemo(() => {
    const lines = b().lines;
    if (b().expanded || lines.length <= COLLAPSE_AT) return { lines, hidden: 0 };
    return { lines: lines.slice(lines.length - TAIL_KEEP), hidden: lines.length - TAIL_KEEP };
  });

  const copy = (text: string) => navigator.clipboard.writeText(text);
  const outputText = () => b().lines.map((l) => lineText(l)).join("\n");

  const rerun = () => {
    const cmd = b().cmd;
    if (!cmd || props.session.mode !== "prompt" || props.session.exited) return;
    setPendingCmd(props.session.id, cmd);
    submitCommand(props.session.id, cmd);
  };

  const cursorRel = () => {
    const c = props.session.cursor;
    if (!b().running || !c || c.block !== b().id || !c.visible) return null;
    return c;
  };

  return (
    <div class="block" classList={{ failed: failed(), running: b().running }}>
      <div class="block-head">
        <span class="block-status">
          <Show when={b().running} fallback={failed() ? "✗" : "❯"}>
            <span class="spinner" />
          </Show>
        </span>
        <span class="block-cmd">
          <Show when={b().cmd} fallback={<em class="muted">shell</em>}>
            <For each={highlightCmd(b().cmd!)}>
              {(t) => <span class={t.cls || undefined}>{t.text}</span>}
            </For>
          </Show>
        </span>
        <span class="block-meta">
          <Show when={b().truncated}>
            <span class="badge warn" title="Output truncated (scrollback limit)">trunc</span>
          </Show>
          <Show when={failed()}>
            <span class="badge err">exit {b().exit}</span>
          </Show>
          <span class="dur">{fmtDur(b().durMs)}</span>
          <span class="actions">
            <Show when={b().cmd}>
              <button title="Copy command" onClick={() => copy(b().cmd!)}>⌘</button>
              <button
                title="Re-run"
                disabled={props.session.mode !== "prompt" || props.session.exited}
                onClick={rerun}
              >↻</button>
            </Show>
            <button title="Copy output" onClick={() => copy(outputText())}>⧉</button>
            <Show when={!b().running}>
              <button title="Delete block" onClick={() => deleteBlock(props.session.id, b().id)}>×</button>
            </Show>
          </span>
        </span>
      </div>
      <Show when={visibleLines().hidden > 0}>
        <button class="expand" onClick={() => toggleExpand(props.session.id, b().id)}>
          … {visibleLines().hidden} satır gizli — tümünü göster
        </button>
      </Show>
      <Show when={b().expanded && b().lines.length > COLLAPSE_AT}>
        <button class="expand" onClick={() => toggleExpand(props.session.id, b().id)}>
          daralt
        </button>
      </Show>
      <div class="block-out">
        <For each={visibleLines().lines}>
          {(runs, i) => {
            const c = cursorRel();
            const rowIdx = () => i() + visibleLines().hidden;
            return (
              <LineRuns
                runs={runs}
                cursorCol={c && c.row === rowIdx() ? c.col : undefined}
              />
            );
          }}
        </For>
        <Show when={b().running && (() => { const c = cursorRel(); return c && c.row >= b().lines.length; })()}>
          <LineRuns runs={[]} cursorCol={cursorRel()!.col} />
        </Show>
      </div>
    </div>
  );
}
