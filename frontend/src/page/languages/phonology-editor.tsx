import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { type Body } from "./phonology-editor/table";
import {
  apply,
  EditorContext,
  initialState,
  PRESETS,
} from "./phonology-editor/state";
import { ColumnHeaders } from "./phonology-editor/header";
import { TableRows } from "./phonology-editor/rows";
import { getMovement, move, serializePath } from "./phonology-editor/path";
import { Controls } from "./phonology-editor/controls";
import { Annotations } from "./phonology-editor/annotations";
import { Help } from "./phonology-editor/help";
import { getKeybindAction } from "./phonology-editor/keybinds";

const PhonologyEditor = ({ body, name }: { body: Body; name: string }) => {
  const [state, dispatch] = React.useReducer(apply, initialState(body, name));
  const tableRef = useRef<HTMLTableElement>(null);

  const [helpOpen, setHelpOpen] = useState(false);

  const handleKeyPress = (e: React.KeyboardEvent) => {
    let movement = getMovement(e);
    if (movement) {
      const newFocus = move(state.body, state.focus, movement);
      if (newFocus) {
        e.preventDefault();
        dispatch({ type: "SetFocus", path: newFocus });
        dispatch({ type: "SetKeybindState", keybindState: "Idle" });
        const serialized = serializePath(state.body, newFocus);
        const selector = `[data-path="${serialized.replaceAll(/"/g, '\\"')}"]`;
        (tableRef.current?.querySelector(selector) as HTMLElement)?.focus();
      }
    }

    const action = getKeybindAction(state, e);
    if (action) {
      dispatch(action);
    }
  };

  // (un)fun fact: in react, onFocus and onBlur subsume both "focus" and "focusin" / "focusout",
  // so we can't distinguish between the two

  const onFocusIn = (e: React.FocusEvent<HTMLTableElement, Element>) => {
    if (tableRef.current?.contains(e.target)) {
      dispatch({ type: "FocusEnter" });
    }
  };

  const onFocusOut = (e: React.FocusEvent<HTMLTableElement, Element>) => {
    if (!tableRef.current?.contains(e.relatedTarget)) {
      dispatch({ type: "FocusLeave" });
    }
  };

  const className =
    "phonology-table" + (state.focusInsideTable ? " focus-inside-table" : "");

  return (
    <EditorContext.Provider value={[state, dispatch]}>
      <div className="phonology-editor">
        <Controls />
        <div className="header-with-actions">
          <h2>{name}</h2>
          <ul>
            <li>
              <Help open={helpOpen} setOpen={setHelpOpen} />
            </li>
          </ul>
        </div>
        <table
          className={className}
          onKeyDown={handleKeyPress}
          onFocus={onFocusIn}
          onBlur={onFocusOut}
          ref={tableRef}
        >
          <thead>
            <ColumnHeaders />
          </thead>
          <tbody>
            <TableRows />
          </tbody>
        </table>
        <Annotations />
      </div>
      <input type="hidden" name="body" value={JSON.stringify(state.body)} />
    </EditorContext.Provider>
  );
};
// --- mount ---

export const mountPhonologyEditor = (
  containerId: string,
  body: Body,
  name: string,
) => {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  const root = ReactDOM.createRoot(container);
  root.render(<PhonologyEditor body={body} name={name} />);
};

if (typeof window !== "undefined") {
  (window as any).mountPhonologyEditor = mountPhonologyEditor;
}
