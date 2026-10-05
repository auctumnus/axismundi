import {
  apply,
  initialState,
  type Action,
  type Snapshot,
} from "./table-editor-core";
import type {
  GrammarBody,
  GrammarCell,
  GrammarFile,
} from "./grammar-table-json";

type GrammarSnapshot = Snapshot<GrammarCell> & { preamble: string };
export type GrammarState = GrammarSnapshot & {
  undoStack: GrammarSnapshot[];
  redoStack: GrammarSnapshot[];
};
export type GrammarAction =
  | Action<GrammarCell>
  | { type: "Import"; file: GrammarFile }
  | { type: "SetPreamble"; preamble: string };

const options = {
  createCell: (): GrammarCell => ({ changes: "" }),
  mergeCells: (anchor: GrammarCell): GrammarCell => ({ ...anchor }),
};
const snapshot = ({
  body,
  preamble,
  focus,
  select,
}: GrammarState): GrammarSnapshot => ({ body, preamble, focus, select });
export const initialGrammarState = (
  body: GrammarBody,
  preamble: string,
): GrammarState => ({
  ...initialState(body),
  preamble,
  undoStack: [],
  redoStack: [],
});

// Grid edits and imports share one history with the preamble, so importing a
// complete paradigm and undoing it cannot leave the wrong shared rules behind.
export const applyGrammar = (
  state: GrammarState,
  action: GrammarAction,
): GrammarState => {
  if (action.type === "Undo") {
    const previous = state.undoStack.at(-1);
    return previous
      ? {
          ...previous,
          undoStack: state.undoStack.slice(0, -1),
          redoStack: [snapshot(state), ...state.redoStack],
        }
      : state;
  }
  if (action.type === "Redo") {
    const next = state.redoStack[0];
    return next
      ? {
          ...next,
          undoStack: [...state.undoStack, snapshot(state)],
          redoStack: state.redoStack.slice(1),
        }
      : state;
  }
  let next: GrammarSnapshot;
  if (action.type === "Import")
    next = { ...action.file, focus: { type: "TopLeft" }, select: null };
  else if (action.type === "SetPreamble")
    next = { ...snapshot(state), preamble: action.preamble };
  else {
    const grid = apply(
      { ...state, undoStack: [], redoStack: [] },
      action,
      options,
    );
    next = {
      body: grid.body,
      focus: grid.focus,
      select: grid.select,
      preamble: state.preamble,
    };
  }
  if (next.body === state.body && next.preamble === state.preamble)
    return next.focus === state.focus && next.select === state.select
      ? state
      : { ...state, focus: next.focus, select: next.select };
  return {
    ...next,
    undoStack: [...state.undoStack, snapshot(state)],
    redoStack: [],
  };
};
