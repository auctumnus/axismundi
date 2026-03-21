import { expect, test } from "bun:test";
import { apply, type EditorState } from "./state";
import type { TablePath } from "./path";

test("FocusEnter sets focusInsideTable to true", () => {
    const initialState: EditorState = {
        body: {
            rows: [],
            columns: [],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
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
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: true,
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
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newFocusPath: TablePath = { type: "Cell", rowPath: [1], colPath: [1] };
    const newState = apply(initialState, { type: "SetFocus", path: newFocusPath });
    expect(newState.focus).toEqual(newFocusPath);
});

test("SetSelect updates the select path", () => {
    const initialState: EditorState = {
        body: {
            rows: [],
            columns: [],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newSelectPath: TablePath = { type: "Cell", rowPath: [1], colPath: [1] };
    const newState = apply(initialState, { type: "SetSelect", path: newSelectPath });
    expect(newState.select).toEqual(newSelectPath);
});

test("SetSelect with null deselects", () => {
    const initialState: EditorState = {
        body: {
            rows: [],
            columns: [],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
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
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "AddPhoneme", phoneme: "a", path: { type: "Cell", rowPath: [0], colPath: [0] } });
    // @ts-ignore
    expect(newState.body.rows[0]!.cells[0].phonemes).toEqual([{ text: "a", annotations: [] }]);
});

test("AddPhoneme with invalid path does not modify state", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "AddPhoneme", phoneme: "a", path: { type: "Cell", rowPath: [1], colPath: [1] } });
    expect(newState).toEqual(initialState);
});

test("RemovePhoneme removes the specified phoneme from the cell", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }, { text: "b", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "RemovePhoneme", path: { type: "Cell", rowPath: [0], colPath: [0] }, index: 0 });
    // @ts-ignore
    expect(newState.body.rows[0]!.cells[0].phonemes).toEqual([{ text: "b", annotations: [] }]);
});

test("RemovePhoneme with invalid path does not modify state", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "RemovePhoneme", path: { type: "Cell", rowPath: [1], colPath: [1] }, index: 0 });
    expect(newState).toEqual(initialState);
});

test("AddNewAnnotation adds a new annotation to the specified phoneme", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: []
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "AddNewAnnotation", annotation: "Test Annotation", path: { type: "Cell", rowPath: [0], colPath: [0] }, phonemeIndex: 0 });
    // @ts-ignore
    expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([0]);
    expect(newState.body.annotations).toEqual(["Test Annotation"]);
});

test("LinkAnnotation links an existing annotation to the specified phoneme", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: ["Existing Annotation"]
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "LinkAnnotation", path: { type: "Cell", rowPath: [0], colPath: [0] }, phonemeIndex: 0, annotationIndex: 0 });
    // @ts-ignore
    expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([0]);
});

test("LinkAnnotation with invalid annotation index does not modify state", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: ["Existing Annotation"]
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "LinkAnnotation", path: { type: "Cell", rowPath: [0], colPath: [0] }, phonemeIndex: 0, annotationIndex: 1 });
    expect(newState).toEqual(initialState);
});

test("LinkAnnotation with invalid path does not modify state", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: ["Existing Annotation"]
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "LinkAnnotation", path: { type: "Cell", rowPath: [1], colPath: [1] }, phonemeIndex: 0, annotationIndex: 0 });
    expect(newState).toEqual(initialState);
});

test("RemoveAnnotation removes the specified annotation from the phoneme", () => {
    const initialState: EditorState = {
        body: {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [{ phonemes: [{ text: "a", annotations: [0] }] }] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
            ],
            annotations: ["Existing Annotation"]
        },
        name: "Test Table",
        focusInsideTable: false,
        focus: { type: "Cell", rowPath: [0], colPath: [0] },
        select: null,
    };
    const newState = apply(initialState, { type: "RemoveAnnotation", path: { type: "Cell", rowPath: [0], colPath: [0] }, phonemeIndex: 0, annotationIndex: 0 });
    // @ts-ignore
    expect(newState.body.rows[0]!.cells[0].phonemes[0].annotations).toEqual([]);
});