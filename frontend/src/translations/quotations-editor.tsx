import React, { useCallback, useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import type { QuotationsEditorElement, QuotationWithWordInfo } from "./types";
import { ControlButton } from "../phonology-editor/controls";
import { Editable, Slate, withReact, type RenderElementProps } from "slate-react";
import { createEditor, Editor, Element, Transforms } from "slate";



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

const initialValue = (initialText: string): QuotationsEditorElement[] => [
  {
    type: 'text',
    text: initialText,
  },
]

const TextElement = ({ attributes, children, element }: RenderElementProps) => {
  return <span {...attributes}>{children}</span>
}

const QuotationElement = ({ attributes, children, element }: RenderElementProps) => {
  return <span {...attributes} className="quotation">{children}</span>
}

const QuotationsEditor = (props: QuotationsEditorProps) => {
  const [editor] = useState(() => withReact(createEditor()))

  const renderElement = useCallback((props: RenderElementProps) => {
    switch (props.element.type) {
      case 'quotation':
        return <QuotationElement {...props} />
      default:
        return <TextElement {...props} />
    }
  }, [])

  const onKeyDown = (event: React.KeyboardEvent) => {
    console.log(event)
    if (event.key === 'k' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();

      Transforms.setNodes(
        editor,
        { type: 'quotation' },
        { match: n => Element.isElement(n) && Editor.isBlock(editor, n)  }
      )
    }
  }

  return (
    <>
      <div className="quotations-editor">
        <QuotationsEditorControls />
        <Slate editor={editor} initialValue={initialValue(props.text)}>
          <Editable
            renderElement={renderElement}
            onKeyDown={onKeyDown}
          />
        </Slate>
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
