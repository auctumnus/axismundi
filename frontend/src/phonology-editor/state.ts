import { createContext, useContext } from "react";
import { getByPath, isPathEqual, isPathPrefixed, move, normalizeHeadingPath, setByPath, type HeadingPath, type Movement, type TablePath } from "./path";
import type { Body, Cell } from "./table";

export const EditorContext = createContext<[EditorState, React.Dispatch<Action>]>(null!);

export const useEditor = (): [EditorState, React.Dispatch<Action>] => {
  return useContext(EditorContext);
}

export const PRESETS: { [key: string]: Body } = (() => {
    const cell = (...phonemes: string[]): Cell => ({
        phonemes: phonemes.map(p => ({ text: p, annotations: [] }))
    });
    return {
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
                { type: "Individual", heading: "Nasal", cells: [cell("m"), cell("n"), cell("nʲ"), cell(), cell()] },
                { type: "Group", heading: "Plosive", rows: [
                    { type: "Individual", heading: "Short", cells: [cell("p"), cell("t"), cell("tʲ"), cell("k"), cell()] },
                    { type: "Individual", heading: "Long", cells: [cell("pː"), cell("tː"), cell("tʲː"), cell("kː"), cell()] },
                ]},
                { type: "Group", heading: "Fricative", rows: [
                    { type: "Individual", heading: "Short", cells: [cell("f"), cell("s"), cell("sʲː"), cell("ʃ"), cell("h")] },
                    { type: "Individual", heading: "Long", cells: [cell("fː"), cell("sː"), cell("sʲː"), cell("ʃː"), cell("hː")] },
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
             annotations: []
        }
    }
})();

export interface EditorState {
  body: Body;
  name: string;
  focusInsideTable: boolean;
  focus: TablePath;
  select: TablePath | null;
}

export const initialState = (body: Body, name: string): EditorState => ({
    body,
    name,
    focusInsideTable: false,
    focus: { type: "Cell", rowPath: [0], colPath: [0] },
    select: null,
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
  | { type: "RemovePhoneme"; path: TablePath; index: number }
  | { type: "AddNewAnnotation"; annotation: string; path: TablePath; phonemeIndex: number }
  | { type: "LinkAnnotation"; path: TablePath; phonemeIndex: number; annotationIndex: number }
  | { type: "RemoveAnnotation"; path: TablePath; phonemeIndex: number; annotationIndex: number }
  | { type: "LoadPreset"; presetName: string }

export const apply = (state: EditorState, action: Action): EditorState => {
    console.log("Applying action", action);
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
        case "LoadPreset": {
            const preset = PRESETS[action.presetName];
            if (!preset) {
                console.warn("LoadPreset action with invalid preset name", action.presetName);
                return state;
            }
            return { ...state, body: preset };
        }
        default:
            return state;
    }
}