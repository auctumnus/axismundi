import { createContext, useContext } from "react";
import { countLeaves, getByPath, headingPathToIndex, isPathEqual, isPathPrefixed, move, normalizeHeadingPath, serializePath, setByPath, type HeadingPath, type Movement, type TablePath } from "./path";
import type { Body, Cell, Column, Row } from "./table";
import { numLeaves } from "./table";

export const EditorContext = createContext<[EditorState, React.Dispatch<Action>]>(null!);

export const useEditor = (): [EditorState, React.Dispatch<Action>] => {
  return useContext(EditorContext);
}

export const PRESETS: { [key: string]: Body } = (() => {
    const cell = (...phonemes: string[]): Cell => ({
        phonemes: phonemes.map(p => ({ text: p, annotations: [] }))
    });
    return {
        "Default": {
            rows: [
                { type: "Individual", heading: "Row 1", cells: [cell(), cell(), cell()] },
                { type: "Individual", heading: "Row 2", cells: [cell(), cell(), cell()] },
                { type: "Individual", heading: "Row 3", cells: [cell(), cell(), cell()] },
            ],
            columns: [
                { type: "Individual", heading: "Column 1" },
                { type: "Individual", heading: "Column 2" },
                { type: "Individual", heading: "Column 3" },
            ],
            annotations: []
        },
        "Simple Consonants": {
            rows: [
                { type: "Individual", heading: "Nasal", cells: [cell("m"), cell("n"), cell("ŋ"), cell()] },
                { type: "Individual", heading: "Plosive", cells: [cell("p", "b"), cell("t", "d"), cell("k"), cell()] },
                { type: "Individual", heading: "Fricative", cells: [cell(), cell("s"), cell(), cell("h")] },
                { type: "Individual", heading: "Approximant", cells: [cell("w"), cell("l"), cell(), cell()] },
            ],
            columns: [
                { type: "Individual", heading: "Labial" },
                { type: "Individual", heading: "Alveolar" },
                { type: "Individual", heading: "Velar" },
                { type: "Individual", heading: "Glottal" },
            ],
            annotations: []
        },
        "Estonian Consonants": {
            rows: [
                { type: "Individual", heading: "Nasal", cells: [cell("m"), {
                        phonemes: [ { text: "n", annotations: [1] } ]
                    }, cell("nʲ"), cell(), cell()] },
                { type: "Group", heading: "Plosive", rows: [
                    { type: "Individual", heading: "Short", cells: [cell("p"), cell("t"), cell("tʲ"), cell("k"), cell()] },
                    { type: "Individual", heading: "Long", cells: [cell("pː"), cell("tː"), cell("tʲː"), cell("kː"), cell()] },
                ]},
                { type: "Group", heading: "Fricative", rows: [
                    { type: "Individual", heading: "Short", cells: [{
                        phonemes: [ { text: "f", annotations: [0] } ]
                    }, cell("s"), cell("sʲː"), {
                        phonemes: [ { text: "ʃ", annotations: [0] } ]
                    }, cell("h")] },
                    { type: "Individual", heading: "Long", cells: [{
                        phonemes: [ { text: "fː", annotations: [0] } ]
                    }, cell("sː"), cell("sʲː"), {
                        phonemes: [ { text: "ʃː", annotations: [0] } ]
                    }, cell("hː")] },
                ]},
                { type: "Individual", heading: "Approximant", cells: [cell("v"), cell("l"), cell("lʲ"), cell("j"), cell()] },
                { type: "Individual", heading: "Trill", cells: [cell(), cell("r"), cell(), cell(), cell()] },
            ],
            columns: [
                { type: "Individual", heading: "Labial" },
                { type: "Group", heading: "Alveolar", columns: [
                    { type: "Individual", heading: "Plain" },
                    { type: "Individual", heading: "Palatalized" },
                ]},
                { type: "Individual", heading: "Velar" },
                { type: "Individual", heading: "Glottal" },
             ],
             annotations: [
                "Appears only in loanwords.",
                "/n/ is realized as velar [ŋ] before a velar consonant.",
             ]
        },
        "Simple Vowels": {
            rows: [
                { type: "Individual", heading: "High", cells: [cell("i"), cell(), cell("u")] },
                { type: "Individual", heading: "Mid", cells: [cell("e"), cell("ə"), cell("o")] },
                { type: "Individual", heading: "Low", cells: [cell(), cell("a"), cell()] },
            ],
            columns: [
                { type: "Individual", heading: "Front" },
                { type: "Individual", heading: "Central" },
                { type: "Individual", heading: "Back" },
            ],
            annotations: []
        },
        "Burmese Vowels": {
            rows: [
                { type: "Individual", heading: "Close", cells: [{ phonemes: [ { text: "i", annotations: [0] } ] }, { phonemes: [ { text: "ĩ", annotations: [0] } ] }, cell(), cell(), { phonemes: [ { text: "u", annotations: [0] } ] }, { phonemes: [ { text: "ũ", annotations: [0] } ] }] },
                { type: "Individual", heading: "Close-mid", cells: [cell("e"), cell(), cell("ə"), cell(), cell("o"), cell()] },
                { type: "Individual", heading: "Open-mid", cells: [cell("ɛ"), cell(), cell(), cell(), cell("ɔ"), cell()] },
                { type: "Individual", heading: "Open", cells: [cell(), cell(), cell("a"), cell("ã"), cell(), cell()] },
            ],
            columns: [
                { type: "Group", heading: "Front", columns: [
                    { type: "Individual", heading: "Oral" },
                    { type: "Individual", heading: "Nasal" },
                ] },
                { type: "Group", heading: "Central", columns: [
                    { type: "Individual", heading: "Oral" },
                    { type: "Individual", heading: "Nasal" },
                ] },
                { type: "Group", heading: "Back", columns: [
                    { type: "Individual", heading: "Oral" },
                    { type: "Individual", heading: "Nasal" },
                ] }
            ],
            annotations: ["Somewhat mid-centralized ([ɪ, ʊ]) in closed syllables."]
        },
        "Sandawe Clicks": {
            columns: [
                { type: "Individual", heading: "Laminal denti-alveolar"},
                { type: "Individual", heading: "Apical post-alveolar" },
                { type: "Individual", heading: "Lateral palatal"},
            ],
            rows: [
                { type: "Individual", heading: "Nasal", cells: [cell("ŋǀ"), cell("ŋǃ"), cell("ŋǁ")] },
                { type: "Individual", heading: "Voiced", cells: [cell("gǀ"), cell("gǃ"), cell("gǁ")] },
                { type: "Individual", heading: "Tenuis", cells: [cell("kǀ"), cell("kǃ"), cell("kǁ")] },
                { type: "Individual", heading: "Aspirated", cells: [cell("kǀʰ"), cell("kǃʰ"), cell("kǁʰ")] },
                { type: "Individual", heading: "Glottalized", cells: [cell("ᵑǀˀ"), cell("ᵑǃˀ"), cell("ᵑǁˀ")] },
            ],
            annotations: []
        },
        "Dutch Diphthongs": {
            columns: [
                {
                    type: "Group",
                    heading: "Front",
                    columns: [
                        {
                            type: "Group",
                            heading: "Unrounded",
                            columns: [
                                { type: "Individual", heading: "Fronting" },
                                { type: "Individual", heading: "Backing" },
                            ]
                        },
                        { type: "Individual", heading: "Rounded" },
                    ]
                },
                {
                    "type": "Group",
                    "heading": "Back",
                    "columns": [
                        { type: "Individual", heading: "Fronting" },
                        { type: "Individual", heading: "Backing" },
                    ]
                }
            ],
            rows: [
                { type: "Individual", heading: "Close", cells: [cell(), cell("iu̯"), cell("yu̯"), cell("ui̯"), cell()] },
                { type: "Individual", heading: "Mid", cells: [{ phonemes: [ { text: "ɛi̯", annotations: [1, 2] }] }, cell("eːu̯"), { phonemes: [ { text: "œy̯", annotations: [1] }] }, cell("ɔi̯", "oːi̯"), { phonemes: [ { text: "ɔu̯", annotations: [0, 1] }] }] },
                { type: "Individual", heading: "Open", cells: [cell(), cell(), cell(), cell("ɑi̯", "aːi̯"), cell()] },
            ],
            annotations: [
                "/ɔu/ has been variously transcribed with ⟨ɔu⟩, ⟨ɑu⟩, and ⟨ʌu⟩.",
                "The starting points of /ɛi, œy, ɔu/ tend to be closer ([ɛɪ, œ̈ʏ, ɔ̈ʊ]) in Belgian Standard Dutch than in Northern Standard Dutch ([ɛ̞ɪ, œ̞̈ʏ, ʌ̞̈ʊ]).",
                "The backness of the starting point of the Belgian Standard Dutch realisation of /ɛi/ has been variously described as front [ɛɪ] and centralised front [ɛ̈ɪ]."
            ]
        }
    }
})();

export type KeybindState =
    | "Idle"
    | "Phoneme"
    | "Annotation"
    | "Heading"

export type ControlModal =
    | "EditRowHeading"
    | "EditColumnHeading"
    | "AddPhoneme"
    | "EditPhoneme"
    | "DeletePhoneme"
    | "AddAnnotation"
    | "LinkAnnotation"
    | "EditAnnotation"
    | "DeleteAnnotation"
    | "LoadPreset"

export interface EditorState {
  body: Body;
  name: string;
  focusInsideTable: boolean;
  focus: TablePath;
  select: TablePath | null;
  undoStack: EditorState[];
  redoStack: EditorState[];
  keybindState: KeybindState;
  pendingModal: ControlModal | null;
}

export const initialState = (body: Body, name: string): EditorState => ({
    body,
    name,
    focusInsideTable: false,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
    undoStack: [],
    redoStack: [],
    keybindState: "Idle",
    pendingModal: null,
});

export const isFocused = (state: EditorState, path: TablePath): boolean => {
    return isPathEqual(state.body, state.focus, path);
}

export const isSelected = (state: EditorState, path: TablePath): boolean => {
    return state.select !== null && isPathEqual(state.body, state.select, path);
}

export const isRowFocused = (state: EditorState, rowPath: HeadingPath): boolean => {
    if(state.focus.type === "RowHeading") {
        const normalized = normalizeHeadingPath(state.body.rows, state.focus.path);
        if (!normalized) {
            return false;
        }
        return isPathPrefixed(normalized, rowPath);
    } else if(state.focus.type === "Cell") {
        return isPathEqual(state.body, state.focus, { type: "Cell", rowPath, colPath: state.focus.colPath });
    }
    return false;
}

export const isColumnFocused = (state: EditorState, colPath: HeadingPath): boolean => {
    if(state.focus.type === "ColumnHeading") {
        const normalized = normalizeHeadingPath(state.body.columns, state.focus.path);
        if (!normalized) {
            return false;
        }
        return isPathPrefixed(normalized, colPath);
    } else if(state.focus.type === "Cell") {
        return isPathEqual(state.body, state.focus, { type: "Cell", rowPath: state.focus.rowPath, colPath });
    }
    return false;
}

export type Action =
  | { type: "FocusEnter" }
  | { type: "FocusLeave" }
  | { type: "SetFocus"; path: TablePath }
  | { type: "SetSelect"; path: TablePath | null }
  | { type: "AddPhoneme"; phoneme: string; path: TablePath }
  | { type: "EditPhoneme"; path: TablePath; index: number; newText: string }
  | { type: "RemovePhoneme"; path: TablePath; index: number }
  | { type: "AddNewAnnotation"; annotation: string; path: TablePath; phonemeIndex: number }
  | { type: "LinkAnnotation"; path: TablePath; phonemeIndex: number; annotationIndex: number }
  | { type: "EditAnnotation"; annotationIndex: number; newText: string }
  | { type: "RemoveAnnotation"; path: TablePath; phonemeIndex: number; annotationIndex: number }
  | { type: "DeleteAnnotationEntirely"; annotationIndex: number }
  | { type: "LoadPreset"; presetName: string }
  // for rows, before means above, after means below; for columns, before means left, after means right
  | { type: "AddHeading", kind: "row" | "column", path: HeadingPath, position: "before" | "after" }
  | { type: "DeleteHeading", kind: "row" | "column", path: HeadingPath }
  | { type: "EditHeading", kind: "row" | "column", path: HeadingPath, newHeading: string }
  | { type: "SplitHeading", kind: "row" | "column", path: HeadingPath }
  | { type: "Undo" }
  | { type: "Redo" }
  | { type: "SetKeybindState", keybindState: KeybindState }
  | { type: "OpenModal", modal: ControlModal }
  | { type: "ClearPendingModal" }

const spliceInHeadings = <T extends Row | Column>(
    headings: T[],
    path: HeadingPath,
    deleteCount: number,
    ...items: T[]
): T[] => {
    if (path.length === 0) return headings;
    if (path.length === 1) {
        const newHeadings = [...headings];
        newHeadings.splice(path[0]!, deleteCount, ...items);
        return newHeadings;
    }
    const [index, ...rest] = path;
    return headings.map((heading, i) => {
        if (i !== index!) return heading;
        if (heading.type === "Group") {
            if ("rows" in heading) {
                return { ...heading, rows: spliceInHeadings(heading.rows as T[], rest, deleteCount, ...items) };
            } else {
                return { ...heading, columns: spliceInHeadings((heading as any).columns as T[], rest, deleteCount, ...items) };
            }
        }
        return heading;
    });
}

const addCellToAllRows = (rows: Row[], colIndex: number): Row[] => {
    return rows.map(row => {
        if (row.type === "Individual") {
            const newCells = [...row.cells];
            newCells.splice(colIndex, 0, { phonemes: [] });
            return { ...row, cells: newCells };
        } else {
            return { ...row, rows: addCellToAllRows(row.rows, colIndex) };
        }
    });
}

const removeCellsFromAllRows = (rows: Row[], colIndex: number, count: number): Row[] => {
    return rows.map(row => {
        if (row.type === "Individual") {
            const newCells = [...row.cells];
            newCells.splice(colIndex, count);
            return { ...row, cells: newCells };
        } else {
            return { ...row, rows: removeCellsFromAllRows(row.rows, colIndex, count) };
        }
    });
}

const collapseRows = (rows: Row[]): Row[] => {
    return rows.map(row => {
        if (row.type !== "Group") return row;
        const collapsed = collapseRows(row.rows);
        if (collapsed.length === 1) {
            const child = collapsed[0]!;
            // keep parent heading, adopt child's structure
            return { ...child, heading: row.heading };
        }
        return { ...row, rows: collapsed };
    });
}

const collapseColumns = (columns: Column[]): Column[] => {
    return columns.map(col => {
        if (col.type !== "Group") return col;
        const collapsed = collapseColumns(col.columns);
        if (collapsed.length === 1) {
            const child = collapsed[0]!;
            return { ...child, heading: col.heading };
        }
        return { ...col, columns: collapsed };
    });
}

const removeAnnotationFromAllRows = (rows: Row[], annotationIndex: number): Row[] => {
    return rows.map(row => {
        if (row.type === "Group") {
            return { ...row, rows: removeAnnotationFromAllRows(row.rows, annotationIndex) };
        }
        return {
            ...row,
            cells: row.cells.map(cell => ({
                phonemes: cell.phonemes.map(p => ({
                    ...p,
                    annotations: p.annotations
                        .filter(i => i !== annotationIndex)
                        .map(i => i > annotationIndex ? i - 1 : i)
                }))
            }))
        };
    });
}

const clampFocus = (body: Body, path: TablePath): TablePath => {
    const inner = (): TablePath => {
        if (getByPath(body, path) !== null) return path;
        return { type: "TopLeft" };
    }
    const newPath = inner();
    // really gross but it works
    requestAnimationFrame(() => {
        const selector = `[data-path="${serializePath(body, newPath).replaceAll(/"/g, '\\"')}"]`;
        const element = document.querySelector(selector) as HTMLElement | null;
        if (element) {
            element.focus();
        }
    })
    return newPath;
}

const clampSelect = (body: Body, path: TablePath): TablePath | null => {
    if (getByPath(body, path) !== null) return path;
    return null;
}

const CANNOT_UNDO = ["Undo", "Redo", "SetFocus", "SetSelect", "FocusEnter", "FocusLeave", "SetKeybindState", "OpenModal", "ClearPendingModal"];

export const apply = (state: EditorState, action: Action): EditorState => {
    console.log("Applying action", action);

    const applyInner = () => {
        switch (action.type) {
            case "FocusEnter":
                return { ...state, focusInsideTable: true };
            case "FocusLeave":
                return { ...state, focusInsideTable: false };
            case "SetFocus":
                return { ...state, focusInsideTable: true, focus: action.path };
            case "SetSelect":
                return { ...state, select: action.path };
            case "AddPhoneme": {
                const { path, phoneme } = action;
                if (path.type !== "Cell") {
                    console.warn("AddPhoneme action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("AddPhoneme action with invalid path", path);
                    return state;
                }
                const newCell = { ...cell, phonemes: [...cell.phonemes, { text: phoneme, annotations: [] }] };
                return { ...state, body: setByPath(state.body, path, newCell) };
            }
            case "RemovePhoneme": {
                const { path, index } = action;
                if (path.type !== "Cell") {
                    console.warn("RemovePhoneme action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("RemovePhoneme action with invalid path", path);
                    return state;
                }
                const newCell = { ...cell, phonemes: cell.phonemes.filter((_, i) => i !== index) };
                return { ...state, body: setByPath(state.body, path, newCell) };
            }
            case "EditPhoneme": {
                const { path, index, newText } = action;
                if (path.type !== "Cell") {
                    console.warn("EditPhoneme action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("EditPhoneme action with invalid path", path);
                    return state;
                }
                const newCell = { ...cell, phonemes: cell.phonemes.map((p, i) =>
                    i === index ? { ...p, text: newText } : p
                )};
                return { ...state, body: setByPath(state.body, path, newCell) };
            }
            case "AddNewAnnotation": {
                const { path, phonemeIndex, annotation } = action;
                if (path.type !== "Cell") {
                    console.warn("AddNewAnnotation action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("AddNewAnnotation action with invalid path", path);
                    return state;
                }
                // add annotation to the table and get its index
                const annotationIndex = state.body.annotations.length;
                const newAnnotations = [...state.body.annotations, annotation];
                // add annotation index to the phoneme
                const newPhoneme = { ...cell.phonemes[phonemeIndex]!, annotations: [...cell.phonemes[phonemeIndex]!.annotations, annotationIndex] };
                const newCell = { ...cell, phonemes: cell.phonemes.map((p, i) => i === phonemeIndex ? newPhoneme : p) };
                const newBody = setByPath(state.body, path, newCell);
                newBody.annotations = newAnnotations;
                return { ...state, body: newBody };
            }
            case "LinkAnnotation": {
                const { path, phonemeIndex, annotationIndex } = action;
                if (annotationIndex < 0 || annotationIndex >= state.body.annotations.length) {
                    console.warn("LinkAnnotation action with invalid annotation index", annotationIndex);
                    return state;
                }
                if (path.type !== "Cell") {
                    console.warn("LinkAnnotation action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("LinkAnnotation action with invalid path", path);
                    return state;
                }
                const newPhoneme = { ...cell.phonemes[phonemeIndex]!, annotations: [...cell.phonemes[phonemeIndex]!.annotations, annotationIndex] };
                const newCell = { ...cell, phonemes: cell.phonemes.map((p, i) => i === phonemeIndex ? newPhoneme : p) };
                return { ...state, body: setByPath(state.body, path, newCell) };
            }
            case "EditAnnotation": {
                const { annotationIndex, newText } = action;
                if (annotationIndex < 0 || annotationIndex >= state.body.annotations.length) {
                    console.warn("EditAnnotation action with invalid annotation index", annotationIndex);
                    return state;
                }
                const newAnnotations = state.body.annotations.map((a, i) => i === annotationIndex ? newText : a);
                return { ...state, body: { ...state.body, annotations: newAnnotations } };
            }
            case "RemoveAnnotation": {
                const { path, phonemeIndex, annotationIndex } = action;
                if (path.type !== "Cell") {
                    console.warn("RemoveAnnotation action applied to non-cell path", path);
                    return state;
                }
                const cell = getByPath(state.body, path);
                if (!cell) {
                    console.warn("RemoveAnnotation action with invalid path", path);
                    return state;
                }
                const newPhoneme = { ...cell.phonemes[phonemeIndex]!, annotations: cell.phonemes[phonemeIndex]!.annotations.filter(i => i !== annotationIndex) };
                const newCell = { ...cell, phonemes: cell.phonemes.map((p, i) => i === phonemeIndex ? newPhoneme : p) };
                return { ...state, body: setByPath(state.body, path, newCell) };
            }
            case "DeleteAnnotationEntirely": {
                const { annotationIndex } = action;
                if (annotationIndex < 0 || annotationIndex >= state.body.annotations.length) {
                    console.warn("DeleteAnnotationEntirely action with invalid annotation index", annotationIndex);
                    return state;
                }
                const newAnnotations = state.body.annotations.filter((_, i) => i !== annotationIndex);
                const newRows = removeAnnotationFromAllRows(state.body.rows, annotationIndex);
                return { ...state, body: { ...state.body, annotations: newAnnotations, rows: newRows } };
            }
            case "LoadPreset": {
                const preset = PRESETS[action.presetName];
                if (!preset) {
                    console.warn("LoadPreset action with invalid preset name", action.presetName);
                    return state;
                }
                return { ...state, body: preset };
            }
            case "AddHeading": {
                const { kind, path, position } = action;
                const lastIndex = path[path.length - 1];
                if (lastIndex === undefined) return state;
                const parentPath = path.slice(0, -1);
                const insertAt = position === "before" ? lastIndex : lastIndex + 1;

                if (kind === "row") {
                    const newRow: Row = {
                        type: "Individual",
                        heading: "New Row",
                        cells: Array.from({ length: numLeaves(state.body.columns) }, () => ({ phonemes: [] }))
                    };
                    const newRows = spliceInHeadings(state.body.rows, [...parentPath, insertAt], 0, newRow);
                    return { ...state, body: { ...state.body, rows: newRows } };
                } else {
                    const colHeading = getByPath(state.body, { type: "ColumnHeading", path });
                    if (!colHeading) return state;
                    const leafIndex = headingPathToIndex(state.body.columns, path);
                    if (leafIndex === null) return state;
                    const insertColIndex = position === "before" ? leafIndex : leafIndex + countLeaves(colHeading as Column);

                    const newCol: Column = { type: "Individual", heading: "New Column" };
                    const newColumns = spliceInHeadings(state.body.columns, [...parentPath, insertAt], 0, newCol);
                    const newRows = addCellToAllRows(state.body.rows, insertColIndex);

                    return { ...state, body: { ...state.body, rows: newRows, columns: newColumns } };
                }
            }
            case "DeleteHeading": {
                const { kind, path } = action;

                let newBody: Body;
                if (kind === "row") {
                    const newRows = collapseRows(spliceInHeadings(state.body.rows, path, 1));
                    newBody = { ...state.body, rows: newRows };
                } else {
                    const colHeading = getByPath(state.body, { type: "ColumnHeading", path });
                    if (!colHeading) return state;
                    const leafIndex = headingPathToIndex(state.body.columns, path);
                    if (leafIndex === null) return state;
                    const leafCount = countLeaves(colHeading as Column);

                    const newColumns = collapseColumns(spliceInHeadings(state.body.columns, path, 1));
                    const newRows = removeCellsFromAllRows(state.body.rows, leafIndex, leafCount);
                    newBody = { ...state.body, rows: newRows, columns: newColumns };
                }

                return {
                    ...state,
                    body: newBody,
                    focus: clampFocus(newBody, state.focus),
                    select: state.select ? clampSelect(newBody, state.select) : null,
                };
            }
            case "EditHeading": {
                const { kind, path, newHeading } = action;
                const tablePath = kind === "row"
                    ? { type: "RowHeading" as const, path }
                    : { type: "ColumnHeading" as const, path };
                return { ...state, body: setByPath(state.body, tablePath, { heading: newHeading }) };
            }
            case "SplitHeading": {
                const { kind, path } = action;

                if (kind === "row") {
                    const heading = getByPath(state.body, { type: "RowHeading", path }) as Row | null;
                    if (!heading) return state;

                    const emptyRow: Row = {
                        type: "Individual",
                        heading: "New Row",
                        cells: Array.from({ length: numLeaves(state.body.columns) }, () => ({ phonemes: [] }))
                    };

                    let newGroup: Row;
                    if (heading.type === "Individual") {
                        // Individual → Group with original (heading cleared) + new empty
                        newGroup = {
                            type: "Group",
                            heading: heading.heading,
                            rows: [{ ...heading, heading: "New Row" }, emptyRow]
                        };
                    } else {
                        // Group → new parent Group with original group + new empty
                        newGroup = {
                            type: "Group",
                            heading: "New Group",
                            rows: [heading, emptyRow]
                        };
                    }

                    const newRows = spliceInHeadings(state.body.rows, path, 1, newGroup);
                    return { ...state, body: { ...state.body, rows: newRows } };
                } else {
                    const heading = getByPath(state.body, { type: "ColumnHeading", path }) as Column | null;
                    if (!heading) return state;
                    const leafIndex = headingPathToIndex(state.body.columns, path);
                    if (leafIndex === null) return state;

                    const newIndividual: Column = { type: "Individual", heading: "New Column" };

                    let newGroup: Column;
                    if (heading.type === "Individual") {
                        newGroup = {
                            type: "Group",
                            heading: heading.heading,
                            columns: [{ ...heading, heading: "New Column" }, newIndividual]
                        };
                    } else {
                        newGroup = {
                            type: "Group",
                            heading: "New Group",
                            columns: [heading, newIndividual]
                        };
                    }

                    const newColumns = spliceInHeadings(state.body.columns, path, 1, newGroup);
                    // add a cell for the new leaf column in every row
                    const insertCellAt = leafIndex + countLeaves(heading);
                    const newRows = addCellToAllRows(state.body.rows, insertCellAt);

                    return { ...state, body: { ...state.body, rows: newRows, columns: newColumns } };
                }
            }
            case "OpenModal":
                return { ...state, pendingModal: action.modal };
            case "ClearPendingModal":
                return { ...state, pendingModal: null };
            case "SetKeybindState":
                return { ...state, keybindState: action.keybindState };
            case "Undo": {
                const undoStack = state.undoStack;
                if (undoStack.length === 0) return state;
                const previousState = undoStack[undoStack.length - 1]!;
                return {
                    ...previousState,
                    focus: clampFocus(previousState.body, state.focus),
                    select: state.select ? clampSelect(previousState.body, state.select) : null,
                    focusInsideTable: state.focusInsideTable,
                    undoStack: undoStack.slice(0, -1),
                    redoStack: [...state.redoStack, state],
                };
            }
            case "Redo": {
                const redoStack = state.redoStack;
                if (redoStack.length === 0) return state;
                const nextState = redoStack[redoStack.length - 1]!;
                return {
                    ...nextState,
                    focus: clampFocus(nextState.body, state.focus),
                    select: state.select ? clampSelect(nextState.body, state.select) : null,
                    focusInsideTable: state.focusInsideTable,
                    undoStack: [...state.undoStack, state],
                    redoStack: redoStack.slice(0, -1),
                };
            }
            default:
                return state;
        }
    }

    const newState = applyInner();
    const isUndoable = !CANNOT_UNDO.includes(action.type);
    if (newState !== state && isUndoable) {
        return { ...newState, undoStack: [...state.undoStack, state], redoStack: [] };
    }
    return newState;
}