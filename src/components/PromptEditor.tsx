import { onCleanup, onMount, createEffect } from "solid-js";
import { EditorView, keymap, placeholder } from "@codemirror/view";
import { EditorState, Prec } from "@codemirror/state";
import { history, historyKeymap, defaultKeymap, insertNewline } from "@codemirror/commands";
import {
  autocompletion,
  completionKeymap,
  acceptCompletion,
  startCompletion,
  completionStatus,
} from "@codemirror/autocomplete";
import { StreamLanguage, syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import type { Session } from "../state";
import { setPendingCmd } from "../state";
import { submitCommand } from "../ipc";
import { makeCompletionSource, getHistory, pushHistory } from "../autocomplete";

const cmTheme = EditorView.theme(
  {
    "&": {
      "background-color": "transparent",
      "font-size": "inherit",
      color: "var(--fg)",
    },
    ".cm-content": {
      "font-family": "var(--mono)",
      padding: "6px 0",
      "caret-color": "var(--accent)",
    },
    ".cm-line": { padding: "0 4px" },
    "&.cm-focused": { outline: "none" },
    ".cm-cursor": { "border-left-color": "var(--accent)" },
    ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": {
      "background-color": "#3e4451 !important",
    },
    ".cm-tooltip": {
      "background-color": "#21252b",
      border: "1px solid #181a1f",
      color: "var(--fg)",
    },
    ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
      "background-color": "#2c313a",
      color: "var(--fg)",
    },
  },
  { dark: true },
);

export function PromptEditor(props: { session: Session; autocomplete: boolean }) {
  let host!: HTMLDivElement;
  let view: EditorView | undefined;
  let histIdx = -1;
  let draft = "";

  const sid = () => props.session.id;

  const submit = (v: EditorView): boolean => {
    const text = v.state.doc.toString();
    if (!text.trim()) return true;
    setPendingCmd(sid(), text);
    pushHistory(text);
    submitCommand(sid(), text);
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: "" } });
    histIdx = -1;
    draft = "";
    return true;
  };

  const histNav = (v: EditorView, dir: 1 | -1): boolean => {
    const hist = getHistory();
    if (!hist.length) return false;
    // Only intercept when the cursor can't move further vertically.
    const sel = v.state.selection.main;
    const line = v.state.doc.lineAt(sel.head);
    if (dir === 1 && line.number !== 1) return false;
    if (dir === -1 && line.number !== v.state.doc.lines) return false;

    if (dir === 1) {
      if (histIdx === -1) draft = v.state.doc.toString();
      if (histIdx >= hist.length - 1) return true;
      histIdx++;
    } else {
      if (histIdx === -1) return false;
      histIdx--;
    }
    const text = histIdx === -1 ? draft : hist[histIdx];
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: text },
      selection: { anchor: text.length },
    });
    return true;
  };

  onMount(() => {
    const exts = [
      history(),
      cmTheme,
      StreamLanguage.define(shell),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      placeholder("komut yaz… (Enter çalıştır, Shift+Enter yeni satır)"),
      EditorView.lineWrapping,
      Prec.highest(
        keymap.of([
          {
            key: "Enter",
            run: (v) => {
              if (completionStatus(v.state) === "active") return acceptCompletion(v);
              return submit(v);
            },
          },
          { key: "Shift-Enter", run: insertNewline },
          {
            key: "Tab",
            run: (v) => {
              if (completionStatus(v.state) === "active") return acceptCompletion(v);
              return startCompletion(v);
            },
          },
          { key: "ArrowUp", run: (v) => histNav(v, 1) },
          { key: "ArrowDown", run: (v) => histNav(v, -1) },
          {
            key: "Ctrl-c",
            run: (v) => {
              v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: "" } });
              histIdx = -1;
              return true;
            },
          },
        ]),
      ),
      keymap.of([...completionKeymap, ...historyKeymap, ...defaultKeymap]),
    ];
    if (props.autocomplete) {
      exts.push(
        autocompletion({
          override: [makeCompletionSource(() => sid())],
          activateOnTyping: true,
          icons: false,
          defaultKeymap: false,
        }),
      );
    }
    view = new EditorView({
      state: EditorState.create({ doc: "", extensions: exts }),
      parent: host,
    });
    view.focus();
  });

  // Refocus when this session becomes the prompt again.
  createEffect(() => {
    if (props.session.mode === "prompt" && !props.session.exited) {
      queueMicrotask(() => view?.focus());
    }
  });

  onCleanup(() => view?.destroy());

  const shortCwd = () => {
    const home = props.session.cwd.replace(/\/+$/, "");
    return home.replace(/^\/Users\/[^/]+/, "~");
  };

  return (
    <div class="prompt">
      <div class="prompt-cwd">
        <span class="tok-path">{shortCwd()}</span>
        <span class="prompt-sym"> ❯</span>
      </div>
      <div class="prompt-editor" ref={host} />
    </div>
  );
}
