import { For, Show, createSignal } from "solid-js";
import { store, setStore, renameSession, removeSession } from "../state";
import { createSession, closeSession } from "../ipc";
import { addSession } from "../state";

export async function newSession() {
  const id = await createSession();
  addSession(id);
}

export function closeSessionUi(id: number) {
  closeSession(id);
  removeSession(id);
}

export function SessionPanel() {
  const [editing, setEditing] = createSignal<number | null>(null);

  return (
    <aside class="panel">
      <div class="panel-head">
        <span class="panel-title">Oturumlar</span>
        <button class="new-btn" title="Yeni oturum (⌘T)" onClick={newSession}>
          +
        </button>
      </div>
      <div class="panel-list">
        <For each={store.sessions}>
          {(s) => (
            <div
              class="sess"
              classList={{ active: store.active === s.id }}
              onClick={() => setStore("active", s.id)}
              onDblClick={() => setEditing(s.id)}
            >
              <span
                class="dot"
                classList={{
                  running: s.mode !== "prompt" && !s.exited,
                  err: !s.exited && s.mode === "prompt" && s.lastExit != null && s.lastExit !== 0,
                  dead: s.exited,
                }}
              />
              <Show
                when={editing() === s.id}
                fallback={<span class="sess-name">{s.name}</span>}
              >
                <input
                  class="sess-rename"
                  value={s.name}
                  ref={(el) => queueMicrotask(() => el.select())}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      renameSession(s.id, e.currentTarget.value);
                      setEditing(null);
                    }
                    if (e.key === "Escape") setEditing(null);
                    e.stopPropagation();
                  }}
                  onBlur={(e) => {
                    renameSession(s.id, e.currentTarget.value);
                    setEditing(null);
                  }}
                />
              </Show>
              <button
                class="sess-close"
                title="Kapat"
                onClick={(e) => {
                  e.stopPropagation();
                  closeSessionUi(s.id);
                }}
              >
                ×
              </button>
            </div>
          )}
        </For>
      </div>
    </aside>
  );
}
