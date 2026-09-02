import { EditorView } from "@codemirror/view";

// Shared by the sound-change runner and grammar-table cells.
export const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "var(--editor-bg)", border: "1px solid var(--editor-border);", borderRadius: "var(--rounding)", overflow: "hidden" },
  ".cm-content": { fontFamily: "var(--editor-font)" },
  ".cm-scroller": { overflow: "auto" },
  ".cm-gutters": { border: "0px solid transparent", padding: "0", paddingLeft: "4px" },
  ".cm-gutters.cm-gutters-before": { borderRightWidth: "0px !important", paddingLeft: "0px !important" },
  ".cm-gutter": { fontFamily: "var(--editor-font)", backgroundColor: "var(--gutter-bg)", color: "var(--gutter-fg)", borderRight: "0px solid var(--gutter-border)", borderTopLeftRadius: "var(--rounding)", borderBottomLeftRadius: "var(--rounding)", userSelect: "none" },
  "& .cm-lineNumbers .cm-gutterElement": { paddingRight: ".5rem !important", paddingLeft: ".5rem !important" },
  ".cm-activeLine": { backgroundColor: "var(--active-line-bg)" },
  "&.cm-focused .cm-selectionBackground, ::selection": { background: "var(--selection-bg-focused) !important" },
  "& .cm-selectionBackground": { background: "var(--selection-bg) !important" },
  ".cm-activeLineGutter": { backgroundColor: "var(--active-line-bg)" },
  ".cm-cursor": { borderLeft: "1.5px solid var(--cursor-color)" },
  "& .cm-tooltip.cm-tooltip-autocomplete > ul": { fontFamily: "var(--editor-font)" },
  "& .cm-tooltip": { border: "1px solid var(--tooltip-border)", borderRadius: "var(--rounding)", backgroundColor: "var(--tooltip-bg)" },
  "& .cm-tooltip-autocomplete ul li": { marginBottom: "0", padding: ".25rem" },
  "& .cm-tooltip-autocomplete ul li[aria-selected]": { backgroundColor: "var(--tooltip-highlight-bg)", color: "var(--tooltip-highlight-fg)" },
  "& .cm-completionIcon": { paddingRight: "0px", marginRight: ".5rem", position: "relative", top: ".15rem" },
  "& .cm-completionIcon-class::after": { display: "none" },
  "& .cm-completionIcon-class": { width: "1rem", height: "1rem", backgroundImage: "var(--icon-class)", backgroundSize: "contain" },
  "& .cm-completionIcon-feature": { width: "1rem", height: "1rem", backgroundImage: "var(--icon-feature)", backgroundSize: "contain" },
  "& .cm-completionIcon-keyword::after": { display: "none" },
  "& .cm-completionIcon-keyword": { width: "1rem", height: "1rem", backgroundImage: "var(--icon-keyword)", backgroundSize: "contain" },
});
