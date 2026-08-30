import type { KeyboardEvent } from "react";
import type { Action, EditorState } from "./state";
import { getByPath, getMovement, normalizeHeadingPath } from "./path";

export const getKeybindAction = (
  state: EditorState,
  e: KeyboardEvent,
): Action | null => {
  const targetPath = state.select
    ? state.select
    : state.focus
      ? state.focus
      : null;

  switch (e.key) {
    case "z":
      if (e.ctrlKey && !e.shiftKey) {
        return { type: "Undo" };
      } else if (e.ctrlKey && e.shiftKey) {
        return { type: "Redo" };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "y":
      if (e.ctrlKey) {
        return { type: "Redo" };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "Enter":
      if (targetPath) {
        switch (targetPath.type) {
          case "Cell":
            const cell = getByPath(state.body, targetPath)!;
            if (state.keybindState === "Phoneme") {
              const cellHasPhoneme = cell.phonemes.length > 0;
              const modal = cellHasPhoneme ? "EditPhoneme" : "AddPhoneme";
              return { type: "OpenModal", modal };
            }
            if (state.keybindState === "Annotation") {
              const cellHasAnnotation = cell.phonemes.some(
                (p) => p.annotations.length > 0,
              );
              const modal = cellHasAnnotation
                ? "EditAnnotation"
                : "AddAnnotation";
              return { type: "OpenModal", modal };
            }
            return { type: "SetKeybindState", keybindState: "Idle" };
          case "RowHeading":
            return { type: "OpenModal", modal: "EditRowHeading" };
          case "ColumnHeading":
            return { type: "OpenModal", modal: "EditColumnHeading" };
        }
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "p":
      return { type: "SetKeybindState", keybindState: "Phoneme" };
    case "h":
      return { type: "SetKeybindState", keybindState: "Heading" };
    case "a":
      switch (state.keybindState) {
        case "Idle":
          return { type: "SetKeybindState", keybindState: "Annotation" };
        case "Phoneme":
          if (targetPath?.type === "Cell") {
            return { type: "OpenModal", modal: "AddPhoneme" };
          }
          break;
        case "Annotation":
          if (targetPath?.type === "Cell") {
            return { type: "OpenModal", modal: "AddAnnotation" };
          }
          break;
      }
      if (targetPath?.type === "RowHeading") {
        const position = e.shiftKey ? "before" : "after";
        const path = normalizeHeadingPath(state.body.rows, targetPath.path);
        return { type: "AddHeading", path: path!, kind: "row", position };
      }
      if (targetPath?.type === "ColumnHeading") {
        const position = e.shiftKey ? "before" : "after";
        const path = normalizeHeadingPath(state.body.columns, targetPath.path);
        return { type: "AddHeading", path: path!, kind: "column", position };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "s":
      if (state.keybindState === "Heading") {
        if (targetPath?.type === "RowHeading") {
          const path = normalizeHeadingPath(state.body.rows, targetPath.path);
          return { type: "SplitHeading", path: path!, kind: "row" };
        } else if (targetPath?.type === "ColumnHeading") {
          const path = normalizeHeadingPath(
            state.body.columns,
            targetPath.path,
          );
          return { type: "SplitHeading", path: path!, kind: "column" };
        }
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "e":
      switch (state.keybindState) {
        case "Phoneme":
          if (targetPath?.type === "Cell") {
            const cell = getByPath(state.body, targetPath)!;
            if (cell.phonemes.length > 0) {
              return { type: "OpenModal", modal: "EditPhoneme" };
            }
          }
          break;
        case "Annotation":
          if (targetPath?.type === "Cell") {
            const cell = getByPath(state.body, targetPath)!;
            if (cell.phonemes.some((p) => p.annotations.length > 0)) {
              return { type: "OpenModal", modal: "EditAnnotation" };
            }
          }
          break;
      }
      if (targetPath?.type === "RowHeading") {
        return { type: "OpenModal", modal: "EditRowHeading" };
      }
      if (targetPath?.type === "ColumnHeading") {
        return { type: "OpenModal", modal: "EditColumnHeading" };
      }

      return { type: "SetKeybindState", keybindState: "Idle" };
    case "d":
      switch (state.keybindState) {
        case "Phoneme":
          if (targetPath?.type === "Cell") {
            const cell = getByPath(state.body, targetPath)!;
            if (cell.phonemes.length > 0) {
              return { type: "OpenModal", modal: "DeletePhoneme" };
            }
          }
          break;
        case "Annotation":
          if (targetPath?.type === "Cell") {
            const cell = getByPath(state.body, targetPath)!;
            if (cell.phonemes.some((p) => p.annotations.length > 0)) {
              return { type: "OpenModal", modal: "DeleteAnnotation" };
            }
          }
          break;
      }
      if (targetPath?.type === "RowHeading") {
        const path = normalizeHeadingPath(state.body.rows, targetPath.path);
        return { type: "DeleteHeading", path: path!, kind: "row" };
      }
      if (targetPath?.type === "ColumnHeading") {
        const path = normalizeHeadingPath(state.body.columns, targetPath.path);
        return { type: "DeleteHeading", path: path!, kind: "column" };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "l":
      if (state.keybindState === "Annotation" && targetPath?.type === "Cell") {
        const cell = getByPath(state.body, targetPath)!;
        if (cell.phonemes.some((p) => p.annotations.length > 0)) {
          return { type: "OpenModal", modal: "LinkAnnotation" };
        }
      }
      break;
    case "m":
      // merge the rectangle between the selected and focused cells
      if (state.select?.type === "Cell" && state.focus?.type === "Cell") {
        return { type: "MergeCells", a: state.select, b: state.focus };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
    case "M":
      if (targetPath?.type === "Cell") {
        return { type: "UnmergeCell", path: targetPath };
      }
      return { type: "SetKeybindState", keybindState: "Idle" };
  }

  return null;
};
