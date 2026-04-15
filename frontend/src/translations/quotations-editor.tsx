import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import type { QuotationWithWordInfo } from "./types";
import { ControlButton } from "../phonology-editor/controls";
import { Slate, withReact } from "slate-react";
import { createEditor } from "slate";



interface QuotationsEditorProps {
  quotations: QuotationWithWordInfo[];
  text: string;
}

const QuotationsEditorControls = () => {
  return (
    <div className="controls-container">
        <div className="controls quotation-controls">
        <span className="controls-header">Quotations</span>
        <ControlButton onClick={() => {}} title="Add at selection" enabled={true}>
            <svg className="icon" aria-hidden><use href="#icon-plus" /></svg>
        </ControlButton>
      </div>
    </div>
  )
}

const initialValue = (initialText: string) => [
  {
    type: 'paragraph',
    children: [{ text: initialText }],
  },
]

const QuotationsEditor = (props: QuotationsEditorProps) => {
  const [editor] = useState(() => withReact(createEditor()))
  return (
    <>
      <div className="quotations-editor">
        <QuotationsEditorControls />
        <Slate editor={editor} initialValue={initialValue(props.text)} />
        <input id="translated_text" name="translated_text" required hidden readOnly value={props.text} />
      </div>
    </>
  )
}


export const mountQuotationsEditor = (
  containerId: string,
  initialProps: QuotationsEditorProps,
) => {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  console.log("Mounting QuotationsEditor with props:", initialProps);

  const root = ReactDOM.createRoot(container);
  root.render(<QuotationsEditor {...initialProps} />);
}

if (typeof window !== "undefined") {
  (window as any).mountQuotationsEditor = mountQuotationsEditor;
}
