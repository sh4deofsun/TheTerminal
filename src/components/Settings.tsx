import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import { listAliases, setAlias, removeAlias, type Alias } from "../ipc";

/** Settings dialog. v1 has a single tab: shell aliases. */
export function Settings(props: { onClose: () => void }) {
  const [aliases, setAliases] = createSignal<Alias[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [filter, setFilter] = createSignal("");
  const [error, setError] = createSignal("");
  const [saved, setSaved] = createSignal<string | null>(null);
  const [newName, setNewName] = createSignal("");
  const [newValue, setNewValue] = createSignal("");

  onMount(async () => {
    try {
      setAliases(await listAliases());
    } catch (e) {
      setError(String(e));
    }
    setLoading(false);
  });

  const shown = createMemo(() => {
    const f = filter().toLowerCase();
    if (!f) return aliases();
    return aliases().filter(
      (a) => a.name.toLowerCase().includes(f) || a.value.toLowerCase().includes(f),
    );
  });

  const save = async (name: string, value: string) => {
    setError("");
    try {
      await setAlias(name, value);
      setAliases((list) => {
        const i = list.findIndex((a) => a.name === name);
        const entry = { name, value, own: true };
        return i < 0 ? [...list, entry].sort((a, b) => a.name.localeCompare(b.name))
                     : list.map((a, k) => (k === i ? entry : a));
      });
      setSaved(name);
      setTimeout(() => setSaved((s) => (s === name ? null : s)), 1500);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  };

  const remove = async (name: string) => {
    setError("");
    try {
      await removeAlias(name);
      setAliases((list) => list.filter((a) => a.name !== name));
    } catch (e) {
      setError(String(e));
    }
  };

  const add = async () => {
    const name = newName().trim();
    const value = newValue().trim();
    if (!name || !value) return;
    if (await save(name, value)) {
      setNewName("");
      setNewValue("");
    }
  };

  return (
    <div class="modal-backdrop" onClick={props.onClose}>
      <div class="modal" onClick={(e) => e.stopPropagation()}>
        <div class="modal-head">
          <span class="modal-title">Ayarlar</span>
          <button class="modal-close" title="Kapat (Esc)" onClick={props.onClose}>
            ×
          </button>
        </div>

        <div class="tabs">
          <button class="tab active">Alias</button>
        </div>

        <div class="modal-body">
          <div class="alias-add">
            <input
              class="alias-name"
              placeholder="ad"
              value={newName()}
              onInput={(e) => setNewName(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
            <span class="alias-eq">=</span>
            <input
              class="alias-value"
              placeholder="komut"
              value={newValue()}
              onInput={(e) => setNewValue(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
            <button class="alias-save" onClick={add}>
              Ekle
            </button>
          </div>

          <input
            class="filter"
            placeholder="alias ara…"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
          />

          <Show when={error()}>
            <div class="alias-error">{error()}</div>
          </Show>

          <Show when={!loading()} fallback={<div class="muted">yükleniyor…</div>}>
            <div class="alias-list">
              <For each={shown()} fallback={<div class="muted">alias yok</div>}>
                {(a) => (
                  <div class="alias-row">
                    <span class="alias-name" classList={{ own: a.own }} title={a.own ? "TheTerminal tarafından yönetiliyor" : "rc dosyalarından geliyor"}>
                      {a.name}
                    </span>
                    <span class="alias-eq">=</span>
                    <input
                      class="alias-value"
                      value={a.value}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") e.currentTarget.blur();
                        if (e.key === "Escape") {
                          e.currentTarget.value = a.value;
                          e.currentTarget.blur();
                        }
                      }}
                      onBlur={(e) => {
                        const v = e.currentTarget.value;
                        if (v !== a.value) save(a.name, v);
                      }}
                    />
                    <span class="alias-flag">{saved() === a.name ? "kaydedildi" : ""}</span>
                    <button
                      class="alias-del"
                      title={a.own ? "Sil" : "Kaldır (rc dosyanız değişmez, unalias ile bastırılır)"}
                      onClick={() => remove(a.name)}
                    >
                      ×
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>

        <div class="modal-foot muted">
          Değişiklikler bundan sonra açılan oturumlarda geçerli olur. rc dosyalarınız
          değiştirilmez; düzenlemeler <code>aliases.zsh</code> dosyasına yazılır.
        </div>
      </div>
    </div>
  );
}
