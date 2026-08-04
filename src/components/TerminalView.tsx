import { For, Show, createSignal, onCleanup, onMount, createEffect, createMemo } from "solid-js";
import type { Session } from "../state";
import { BlockView } from "./BlockView";
import { PromptEditor } from "./PromptEditor";
import { AltScreen } from "./AltScreen";
import { encodeKey } from "../keys";
import { writeSession, resizeSession } from "../ipc";
import { lineText } from "./Runs";

export function TerminalView(props: { session: Session; autocomplete: boolean }) {
  let scroller!: HTMLDivElement;
  let container!: HTMLDivElement;
  let cell!: HTMLSpanElement;
  const [filter, setFilter] = createSignal("");
  let stickBottom = true;

  const s = () => props.session;

  // --- PTY sizing from measured cell metrics ---
  const doResize = () => {
    if (!container || !cell) return;
    const r = cell.getBoundingClientRect();
    const cw = r.width / 10; // span holds 10 chars
    const ch = r.height;
    if (cw <= 0 || ch <= 0) return;
    const cols = Math.max(2, Math.floor((container.clientWidth - 24) / cw));
    const rows = Math.max(2, Math.floor((container.clientHeight - 8) / ch));
    resizeSession(s().id, cols, rows);
  };

  onMount(() => {
    const ro = new ResizeObserver(doResize);
    ro.observe(container);
    doResize();
    onCleanup(() => ro.disconnect());
  });

  // --- Raw input passthrough (running / alt-screen modes) ---
  const rawMode = () => s().mode !== "prompt" && !s().exited;

  const onKey = (e: KeyboardEvent) => {
    if (!rawMode()) return;
    // Don't steal input aimed at the filter box.
    if ((e.target as HTMLElement)?.tagName === "INPUT") return;
    const bytes = encodeKey(e, s().appCursor);
    if (bytes != null) {
      e.preventDefault();
      writeSession(s().id, bytes);
    }
  };

  const onPaste = (e: ClipboardEvent) => {
    if (!rawMode()) return;
    const text = e.clipboardData?.getData("text");
    if (!text) return;
    e.preventDefault();
    const payload = s().bracketed ? `\x1b[200~${text}\x1b[201~` : text;
    writeSession(s().id, payload);
  };

  onMount(() => {
    window.addEventListener("keydown", onKey);
    window.addEventListener("paste", onPaste);
    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("paste", onPaste);
    });
  });

  // --- Autoscroll: stick to bottom unless the user scrolled up ---
  const onScroll = () => {
    stickBottom = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 40;
  };
  createEffect(() => {
    // Track content growth.
    const blocks = s().blocks;
    blocks.length;
    const last = blocks[blocks.length - 1];
    last?.lines.length;
    s().mode;
    if (stickBottom) queueMicrotask(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  const visibleBlocks = createMemo(() => {
    const f = filter().toLowerCase();
    if (!f) return s().blocks;
    return s().blocks.filter(
      (b) =>
        b.cmd?.toLowerCase().includes(f) ||
        b.lines.some((l) => lineText(l).toLowerCase().includes(f)),
    );
  });

  return (
    <div class="termview" ref={container}>
      <span class="cell-probe" ref={cell} aria-hidden="true">MMMMMMMMMM</span>
      <Show
        when={s().mode !== "alt"}
        fallback={<AltScreen session={s()} />}
      >
        <div class="toolbar">
          <input
            class="filter"
            placeholder="bloklarda ara…"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
          />
          <Show when={s().exited}>
            <span class="badge err">oturum kapandı</span>
          </Show>
        </div>
        <div class="blocks" ref={scroller} onScroll={onScroll}>
          <For each={visibleBlocks()}>
            {(b) => <BlockView block={b} session={s()} />}
          </For>
          <Show when={s().mode === "prompt" && !s().exited && !filter()}>
            <PromptEditor session={s()} autocomplete={props.autocomplete} />
          </Show>
        </div>
      </Show>
    </div>
  );
}
