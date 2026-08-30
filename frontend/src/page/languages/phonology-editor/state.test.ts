import { expect, test } from "bun:test";
import { apply, type EditorState } from "./state";
import type { TablePath } from "./path";

test("FocusEnter sets focusInsideTable to true", () => {
  const initialState: EditorState = {
    body: {
      rows: [],
      columns: [],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, { type: "FocusEnter" });
  expect(newState.focusInsideTable).toBe(true);
});

test("FocusLeave sets focusInsideTable to false", () => {
  const initialState: EditorState = {
    body: {
      rows: [],
      columns: [],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: true,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, { type: "FocusLeave" });
  expect(newState.focusInsideTable).toBe(false);
});

test("SetFocus updates the focus path", () => {
  const initialState: EditorState = {
    body: {
      rows: [],
      columns: [],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newFocusPath: TablePath = { type: "Cell", rowPath: [1], colPath: [1] };
  const newState = apply(initialState, {
    type: "SetFocus",
    path: newFocusPath,
  });
  expect(newState.focus).toEqual(newFocusPath);
});

test("SetSelect updates the select path", () => {
  const initialState: EditorState = {
    body: {
      rows: [],
      columns: [],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newSelectPath: TablePath = { type: "Cell", rowPath: [1], colPath: [1] };
  const newState = apply(initialState, {
    type: "SetSelect",
    path: newSelectPath,
  });
  expect(newState.select).toEqual(newSelectPath);
});

test("SetSelect with null deselects", () => {
  const initialState: EditorState = {
    body: {
      rows: [],
      columns: [],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: { type: "Cell", rowPath: [1], colPath: [1] },
  };
  const newState = apply(initialState, { type: "SetSelect", path: null });
  expect(newState.select).toBeNull();
});

test("AddPhoneme adds a phoneme to the specified cell", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        { type: "Individual", heading: "Row 1", cells: [{ phonemes: [] }] },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "AddPhoneme",
    phoneme: "a",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
  });
  // @ts-ignore
  expect(newState.body.rows[0]!.cells[0].phonemes).toEqual([
    { text: "a", annotations: [] },
  ]);
});

test("AddPhoneme with invalid path does not modify state", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        { type: "Individual", heading: "Row 1", cells: [{ phonemes: [] }] },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "AddPhoneme",
    phoneme: "a",
    path: { type: "Cell", rowPath: [1], colPath: [1] },
  });
  expect(newState).toEqual(initialState);
});

test("RemovePhoneme removes the specified phoneme from the cell", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [
            {
              phonemes: [
                { text: "a", annotations: [] },
                { text: "b", annotations: [] },
              ],
            },
          ],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "RemovePhoneme",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
    index: 0,
  });
  // @ts-ignore
  expect(newState.body.rows[0]!.cells[0].phonemes).toEqual([
    { text: "b", annotations: [] },
  ]);
});

test("RemovePhoneme with invalid path does not modify state", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "RemovePhoneme",
    path: { type: "Cell", rowPath: [1], colPath: [1] },
    index: 0,
  });
  expect(newState).toEqual(initialState);
});

test("AddNewAnnotation adds a new annotation to the specified phoneme", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: [],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "AddNewAnnotation",
    annotation: "Test Annotation",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
    phonemeIndex: 0,
  });
  // @ts-ignore
  expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([0]);
  expect(newState.body.annotations).toEqual(["Test Annotation"]);
});

test("LinkAnnotation links an existing annotation to the specified phoneme", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: ["Existing Annotation"],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "LinkAnnotation",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
    phonemeIndex: 0,
    annotationIndex: 0,
  });
  // @ts-ignore
  expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([0]);
});

test("LinkAnnotation with invalid annotation index does not modify state", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: ["Existing Annotation"],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "LinkAnnotation",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
    phonemeIndex: 0,
    annotationIndex: 1,
  });
  expect(newState).toEqual(initialState);
});

test("LinkAnnotation with invalid path does not modify state", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: ["Existing Annotation"],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "LinkAnnotation",
    path: { type: "Cell", rowPath: [1], colPath: [1] },
    phonemeIndex: 0,
    annotationIndex: 0,
  });
  expect(newState).toEqual(initialState);
});

test("RemoveAnnotation removes the specified annotation from the phoneme", () => {
  const initialState: EditorState = {
    body: {
      rows: [
        {
          type: "Individual",
          heading: "Row 1",
          cells: [{ phonemes: [{ text: "a", annotations: [0] }] }],
        },
      ],
      columns: [{ type: "Individual", heading: "Column 1" }],
      annotations: ["Existing Annotation"],
    },
    name: "Test Table",
    focusInsideTable: false,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
    pendingPhonemeIndex: null,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
  };
  const newState = apply(initialState, {
    type: "RemoveAnnotation",
    path: { type: "Cell", rowPath: [0], colPath: [0] },
    phonemeIndex: 0,
    annotationIndex: 0,
  });
  // @ts-ignore
  expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([]);
});

// --- merged cells ---

import type { Body, Cell } from "./table";
import { cellColspan, cellRowspan, leafCellRows } from "./spans";

const mkCell = (...phonemes: string[]): Cell => ({
  phonemes: phonemes.map((p) => ({ text: p, annotations: [] })),
});

const mkState = (body: Body): EditorState => ({
  body,
  name: "Test Table",
  focusInsideTable: false,
  focus: { type: "Cell", rowPath: 0, colPath: 0 },
  select: null,
  undoStack: [],
  redoStack: [],
  keybindState: "Idle",
  pendingModal: null,
  pendingPhonemeIndex: null,
});

const grid2x3 = (): Body => ({
  rows: [
    {
      type: "Individual",
      heading: "R1",
      cells: [mkCell("a"), mkCell("b"), mkCell("c")],
    },
    {
      type: "Individual",
      heading: "R2",
      cells: [mkCell("d"), mkCell("e"), mkCell("f")],
    },
  ],
  columns: [
    { type: "Individual", heading: "C1" },
    { type: "Individual", heading: "C2" },
    { type: "Individual", heading: "C3" },
  ],
  annotations: [],
});

test("MergeCells merges the rectangle and gathers phonemes into the anchor", () => {
  const state = mkState(grid2x3());
  const newState = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 1, colPath: 1 },
  });
  const cells = leafCellRows(newState.body.rows);
  const anchor = cells[0]![0]!;
  expect(cellRowspan(anchor)).toBe(2);
  expect(cellColspan(anchor)).toBe(2);
  expect(anchor.phonemes.map((p) => p.text)).toEqual(["a", "b", "d", "e"]);
  // covered cells are emptied
  expect(cells[0]![1]!.phonemes).toEqual([]);
  expect(cells[1]![0]!.phonemes).toEqual([]);
  expect(cells[1]![1]!.phonemes).toEqual([]);
  // untouched cells stay
  expect(cells[0]![2]!.phonemes.map((p) => p.text)).toEqual(["c"]);
  // focus and select land on the anchor
  expect(newState.focus).toEqual({ type: "Cell", rowPath: 0, colPath: 0 });
  expect(newState.select).toEqual({ type: "Cell", rowPath: 0, colPath: 0 });
});

test("MergeCells expands over a partially overlapped merge", () => {
  const state = mkState(grid2x3());
  // first merge (0,1)-(1,1) vertically
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 1 },
    b: { type: "Cell", rowPath: 1, colPath: 1 },
  });
  // then merge (0,0) with the merged cell; must swallow both its rows
  const newState = apply(merged, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 0, colPath: 1 },
  });
  const anchor = leafCellRows(newState.body.rows)[0]![0]!;
  expect(cellRowspan(anchor)).toBe(2);
  expect(cellColspan(anchor)).toBe(2);
});

test("MergeCells with the same cell twice is a no-op", () => {
  const state = mkState(grid2x3());
  const newState = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 0, colPath: 0 },
  });
  expect(newState.body).toEqual(state.body);
});

test("UnmergeCell resets spans and leaves phonemes on the anchor", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 1, colPath: 1 },
  });
  const newState = apply(merged, {
    type: "UnmergeCell",
    path: { type: "Cell", rowPath: 0, colPath: 0 },
  });
  const cells = leafCellRows(newState.body.rows);
  expect(cellRowspan(cells[0]![0]!)).toBe(1);
  expect(cellColspan(cells[0]![0]!)).toBe(1);
  expect(cells[0]![0]!.phonemes.map((p) => p.text)).toEqual([
    "a",
    "b",
    "d",
    "e",
  ]);
  expect(cells[1]![1]!.phonemes).toEqual([]);
});

test("MergeCells then Undo restores the original body", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 1, colPath: 1 },
  });
  const undone = apply(merged, { type: "Undo" });
  expect(undone.body).toEqual(state.body);
});

test("adding a column inside a merge widens it", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 0, colPath: 1 },
  });
  // add a column after C1 -> lands inside the merge
  const newState = apply(merged, {
    type: "AddHeading",
    kind: "column",
    path: [0],
    position: "after",
  });
  const cells = leafCellRows(newState.body.rows);
  expect(cells[0]!.length).toBe(4);
  expect(cellColspan(cells[0]![0]!)).toBe(3);
});

test("deleting a column inside a merge shrinks it", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 0, colPath: 1 },
  });
  const newState = apply(merged, {
    type: "DeleteHeading",
    kind: "column",
    path: [1],
  });
  const cells = leafCellRows(newState.body.rows);
  expect(cells[0]!.length).toBe(2);
  expect(cellColspan(cells[0]![0]!)).toBe(1);
});

test("adding a row inside a vertical merge stretches it", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 1, colPath: 0 },
  });
  // add a row after R1 -> lands inside the merge
  const newState = apply(merged, {
    type: "AddHeading",
    kind: "row",
    path: [0],
    position: "after",
  });
  const cells = leafCellRows(newState.body.rows);
  expect(cells.length).toBe(3);
  expect(cellRowspan(cells[0]![0]!)).toBe(3);
});

test("deleting a row spanned by a merge shrinks it", () => {
  const state = mkState(grid2x3());
  const merged = apply(state, {
    type: "MergeCells",
    a: { type: "Cell", rowPath: 0, colPath: 0 },
    b: { type: "Cell", rowPath: 1, colPath: 0 },
  });
  const newState = apply(merged, {
    type: "DeleteHeading",
    kind: "row",
    path: [1],
  });
  const cells = leafCellRows(newState.body.rows);
  expect(cells.length).toBe(1);
  expect(cellRowspan(cells[0]![0]!)).toBe(1);
});
