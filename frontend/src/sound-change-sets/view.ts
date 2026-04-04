import {EditorState} from "@codemirror/state"
import {EditorView, lineNumbers} from "@codemirror/view"
import {syntaxHighlighting, defaultHighlightStyle} from "@codemirror/language"
import {lexurgy} from "../lexurgy-language"
import {theme} from "../sound-change-runner"

window.addEventListener("DOMContentLoaded", () => {
    const pre = document.querySelector(".sound-change-set pre");
    if (!pre) return;

    const doc = pre.textContent ?? "";

    const state = EditorState.create({
        doc,
        extensions: [
            EditorState.readOnly.of(true),
            EditorView.editable.of(false),
            lineNumbers(),
            lexurgy(),
            syntaxHighlighting(defaultHighlightStyle),
            theme,
        ],
    });

    const view = new EditorView({ state });
    pre.replaceWith(view.dom);
});
