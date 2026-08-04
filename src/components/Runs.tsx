import { For, Show } from "solid-js";
import type { Run } from "../ipc";

/** One terminal line: styled runs → spans. st bits: 1 bold 2 italic 4 underline 8 dim 16 strike. */
export function LineRuns(props: { runs?: Run[]; cursorCol?: number }) {
  return (
    <div class="tline">
      <Show when={props.runs && props.runs.length > 0} fallback={<span>&nbsp;</span>}>
        <For each={props.runs}>
          {(r) => {
            const st = r.st ?? 0;
            return (
              <span
                style={{
                  color: r.fg,
                  "background-color": r.bg,
                  "font-weight": st & 1 ? "bold" : undefined,
                  "font-style": st & 2 ? "italic" : undefined,
                  "text-decoration":
                    (st & 4 ? "underline " : "") + (st & 16 ? "line-through" : "") || undefined,
                  opacity: st & 8 ? 0.6 : undefined,
                }}
              >
                {r.x}
              </span>
            );
          }}
        </For>
      </Show>
      <Show when={props.cursorCol != null}>
        <span class="caret" style={{ left: `${props.cursorCol}ch` }} />
      </Show>
    </div>
  );
}

export function lineText(runs?: Run[]): string {
  return runs ? runs.map((r) => r.x).join("") : "";
}
