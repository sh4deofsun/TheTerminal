import { Show, onMount, onCleanup, createSignal } from "solid-js";
import "./App.css";
import { SessionPanel, newSession, closeSessionUi } from "./components/SessionPanel";
import { TerminalView } from "./components/TerminalView";
import { Settings } from "./components/Settings";
import { store, setStore, activeSession, applyEvents, clearBlocks, modalOpen, setModalOpen } from "./state";
import { getConfig, onSessEvt, readHistory } from "./ipc";
import { setHistory } from "./autocomplete";

function App() {
  const [autocomplete, setAutocomplete] = createSignal(true);

  onMount(async () => {
    const cfg = await getConfig();
    document.documentElement.style.setProperty("--mono", cfg.font_family);
    document.documentElement.style.setProperty("--font-size", `${cfg.font_size}px`);
    setAutocomplete(cfg.autocomplete);

    readHistory().then(setHistory).catch(() => {});

    const unlisten = await onSessEvt((e) => applyEvents(e.id, e.evs));
    onCleanup(() => unlisten());

    if (store.sessions.length === 0) await newSession();
  });

  // Global shortcuts: ⌘T new, ⌘W close, ⌘K clear, ⌘, settings, ⌘1-9 switch.
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape" && modalOpen()) {
      e.preventDefault();
      setModalOpen(false);
      return;
    }
    if (!e.metaKey) return;
    if (e.key === ",") {
      e.preventDefault();
      setModalOpen(true);
      return;
    }
    if (modalOpen()) return;
    if (e.key === "t") {
      e.preventDefault();
      newSession();
    } else if (e.key === "w") {
      e.preventDefault();
      if (store.active != null) closeSessionUi(store.active);
    } else if (e.key === "k") {
      e.preventDefault();
      if (store.active != null) clearBlocks(store.active);
    } else if (/^[1-9]$/.test(e.key)) {
      const s = store.sessions[parseInt(e.key, 10) - 1];
      if (s) {
        e.preventDefault();
        setStore("active", s.id);
      }
    }
  };
  onMount(() => {
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  return (
    <div class="app">
      <SessionPanel onSettings={() => setModalOpen(true)} />
      <main class="main">
        <Show
          when={activeSession()}
          fallback={<div class="empty">Oturum yok — ⌘T ile yeni oturum aç</div>}
        >
          {(s) => <TerminalView session={s()} autocomplete={autocomplete()} />}
        </Show>
      </main>
      <Show when={modalOpen()}>
        <Settings onClose={() => setModalOpen(false)} />
      </Show>
    </div>
  );
}

export default App;
