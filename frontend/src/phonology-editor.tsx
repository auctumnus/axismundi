import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import {type Body} from "./phonology-editor/table";
import { apply, EditorContext, initialState, PRESETS } from "./phonology-editor/state";
import { ColumnHeaders } from "./phonology-editor/header";
import { TableRows } from "./phonology-editor/rows";

const PhonologyEditor = ({ body, name }: { body: Body; name: string }) => {
  const [state, dispatch] = React.useReducer(apply, initialState(body, name));

  useEffect(() => {
    const focused = document.querySelector<HTMLElement>('[tabindex="0"]');
    console.log("Focused element:", focused);
    focused?.focus();
  }, [state.focus]);

  return (
    <EditorContext.Provider value={[state, dispatch]}>
      <div className="phonology-editor">
        <table className="phonology-table">
          <thead>
            <ColumnHeaders />
          </thead>
          <tbody>
            <TableRows />
          </tbody>
        </table>
      </div>
    </EditorContext.Provider>
  );
}
// --- mount ---

export const mountPhonologyEditor = (
  containerId: string,
  body: Body,
  name: string,
) => {
  console.log("meow")
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  console.log(container);

  const root = ReactDOM.createRoot(container);
  root.render(<PhonologyEditor body={PRESETS["Estonian Consonants"]!} name={name} />);

  console.log(body);
}

if (typeof window !== "undefined") {
  (window as any).mountPhonologyEditor = mountPhonologyEditor;
}

