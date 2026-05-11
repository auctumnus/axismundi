import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import {markdown} from "@codemirror/lang-markdown"
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { EditorSelection } from "@codemirror/state";

const theme = EditorView.theme({
    "&": {
        height: "15rem",
        backgroundColor: "var(--input-background)",
        border: "1px solid var(--input-border);",
        borderTop: "none",
        borderRadius: "var(--rounding)",
        borderTopLeftRadius: "0px",
        borderTopRightRadius: "0px",
        overflow: "hidden",
        transition: "border-color 0.25s, box-shadow 0.25s",
        outline: "0px solid transparent",
        padding: ".5rem"
    },
    ".cm-content": {
        fontFamily: "var(--font-normal)",
        padding: "0px",
    },
    ".cm-line": {
        padding: "0px",
    },
    ".cm-scroller": { overflow: "auto" },
    "&.cm-focused .cm-selectionBackground, ::selection": {
        //background: "var(--selection-bg-focused) !important",
    },
    "& .cm-selectionBackground": {
        background: "var(--selection-bg) !important",
    },
    ".cm-cursor": {
        borderLeft: "1.5px solid var(--cursor-color)",
    },
    "&.cm-focused": {
        // boxShadow: `
        //     1px 1px 0px 1px var(--focus-ring),
        //     -2px 1px 0px 1px var(--focus-ring)
        // `,
        border: "1px solid var(--input-border-focus)",
        borderTop: "none",
        backgroundColor: "var(--input-background-focus)",
        outline: "2px solid var(--focus-ring)",
    }
});

const makeControls = (view: EditorView) => {
    const wrap = (char: string) => {
        view.dispatch(view.state.changeByRange(range => ({
            changes: [
                { from: range.from, insert: char },
                { from: range.to, insert: char },
            ],
            range: EditorSelection.range(range.from, range.to + (char.length * 2))
        })));
    }

    const makeButton = (icon: string, desc: string, onClick: () => void) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'markdown-control';
        button.title = desc;
        button.innerHTML = `<svg class="icon"><use href="#icon-${icon}"></use></svg>`;
        button.addEventListener('click', onClick);
        return button;
    }

    const controls = document.createElement('div');
    controls.className = 'markdown-controls';
    const buttons = [
        { icon: 'bold', desc: 'Bold (Ctrl+B)', action: () => wrap('**') },
        { icon: 'italic', desc: 'Italic (Ctrl+I)', action: () => wrap('*') },
        { icon: 'strikethrough', desc: 'Strikethrough (Ctrl+Shift+S)', action: () => wrap('~~') },
        { icon: 'link', desc: 'Link (Ctrl+K)', action: () => {
            view.dispatch(view.state.changeByRange(range => {
                const changes = [
                    { from: range.from, insert: '[' },
                    { from: range.to, insert: '](url)' },
                ];
                return {
                    changes,
                    range: EditorSelection.range(range.from + 1, range.to + 1),
                };
            }));
        } },
        { icon: 'code', desc: 'Inline Code (Ctrl+E)', action: () => {
            // wrap in `` if single line, otherwise wrap in code block
            const selectedText = view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);
            if (selectedText.includes('\n')) {
                view.dispatch(view.state.changeByRange(range => {
                    const changes = [
                        { from: range.from, insert: '```\n' },
                        { from: range.to, insert: '\n```' },
                    ];
                    return {
                        changes,
                        range: EditorSelection.range(range.from + 4, range.to + 4),
                    };
                }));
            } else {
                wrap('`');
            }
        }},
        { icon: 'unordered-list', desc: 'Unordered List (Ctrl+Shift+L)', action: () => {
            // place - at the start of each selected line
            view.dispatch(view.state.changeByRange(range => {
                const changes = [];
                const fromLine = view.state.doc.lineAt(range.from);
                const toLine = view.state.doc.lineAt(range.to);
                for (let lineNum = fromLine.number; lineNum <= toLine.number; lineNum++) {
                    const line = view.state.doc.line(lineNum);
                    changes.push({ from: line.from, insert: '- ' });
                }
                const numLines = toLine.number - fromLine.number + 1;
                return {
                    changes,
                    range: EditorSelection.range(range.from + 2, range.to + 2 * numLines),
                };
            }));
        } },
        { icon: 'ordered-list', desc: 'Ordered List (Ctrl+Shift+O)', action: () => {
            // place 1. at the start of each selected line, incrementing the number for each line
            view.dispatch(view.state.changeByRange(range => {
                const changes = [];
                const fromLine = view.state.doc.lineAt(range.from);
                const toLine = view.state.doc.lineAt(range.to);
                for (let lineNum = fromLine.number; lineNum <= toLine.number; lineNum++) {
                    const line = view.state.doc.line(lineNum);
                    changes.push({ from: line.from, insert: `${lineNum - fromLine.number + 1}. ` });
                }
                const numLines = toLine.number - fromLine.number + 1;
                return {
                    changes,
                    range: EditorSelection.range(range.from + 3, range.to + 3 * numLines),
                };
            }));
        } },
    ];

    buttons.forEach(({ icon, desc, action }) => {
        const button = makeButton(icon, desc, action);
        controls.appendChild(button);
    });

    return controls;
}

const mdHighlight = HighlightStyle.define([
    { tag: tags.monospace, fontFamily: "var(--font-monospace)" },
    { tag: tags.processingInstruction, color: "var(--foreground-secondary)" },
    { tag: tags.strong, fontWeight: "bold" },
    { tag: tags.emphasis, fontStyle: "italic" },
    { tag: tags.strikethrough, textDecoration: "line-through" },
    { tag: tags.link, color: "var(--color-sky-500)", textDecoration: "underline" },
]);

const mountMarkdownEditor = (target: string, labelID: string) => {
    const textarea = document.querySelector(target);
    if (!textarea) {
        console.error(`Target element "${target}" not found.`);
        return;
    }

    if (!(textarea instanceof HTMLTextAreaElement)) {
        console.error(`Target element "${target}" is not a textarea.`);
        return;
    }

    const value = textarea.value;
    const name = textarea.name;

    const container = document.createElement('div');
    container.className = 'markdown-editor';
    textarea.replaceWith(container);

    const input = document.createElement('input');
    input.type = 'hidden';
    input.name = name;
    input.value = value;
    container.appendChild(input);

    const view = new EditorView({
        doc: value,
        parent: container,
        extensions: [
            theme,
            history(),
            markdown(),
            syntaxHighlighting(mdHighlight),
            keymap.of([
                ...defaultKeymap,
                ...historyKeymap,
            ]),
            EditorView.updateListener.of((update) => {
                if (update.docChanged) {
                    const newValue = update.state.doc.toString();
                    input.value = newValue;
                }
            })   
        ]
    });

    view.contentDOM.setAttribute("aria-labelledby", labelID);
    view.contentDOM.setAttribute("role", "textbox");
    view.contentDOM.setAttribute("aria-multiline", "true");

    const controls = makeControls(view);
    container.insertBefore(controls, view.dom);
}

if (window) {
    // @ts-ignore
    window.mountMarkdownEditor = mountMarkdownEditor;
}