import { For } from "solid-js";
import type { Session } from "../state";
import { LineRuns } from "./Runs";

/** Full-screen TUI view (vim, htop…): dense grid + cursor overlay. */
export function AltScreen(props: { session: Session }) {
  const scr = () => props.session.screen;
  return (
    <div class="altscreen">
      <For each={[...Array(scr().lines).keys()]}>
        {(i) => (
          <LineRuns
            runs={scr().rows[i]}
            cursorCol={scr().cursor[0] === i ? scr().cursor[1] : undefined}
          />
        )}
      </For>
    </div>
  );
}
