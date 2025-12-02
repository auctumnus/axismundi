import { createRoot } from 'react-dom/client';
// Import React dependencies.
import { useState, useCallback } from 'react'
// Import the Slate editor factory.
import { createEditor, Editor, Range, Text } from 'slate'

import type { BaseEditor, Descendant } from 'slate'
import { ReactEditor, useSlate } from 'slate-react'

// Import the Slate components and React plugin.
import { Slate, Editable, withReact } from 'slate-react'
import './slate-editor.css'

type AnnotationData = {
  note: string
}

type CustomElement = { type: 'paragraph'; children: CustomText[] }
type CustomText = { text: string; annotated?: AnnotationData }

declare module 'slate' {
  interface CustomTypes {
    Editor: BaseEditor & ReactEditor
    Element: CustomElement
    Text: CustomText
  }
}

// Initial value for the editor
const initialValue: Descendant[] = [
  {
    type: 'paragraph',
    children: [{ text: 'Start typing your annotation here...' }],
  },
]

// Custom editor commands
const CustomEditor = {
  isAnnotated(editor: Editor) {
    const marks = Editor.marks(editor)
    return marks ? marks.annotated !== undefined : false
  },

  hasSelection(editor: Editor) {
    return editor.selection && !Range.isCollapsed(editor.selection)
  },

  hasAnnotationInSelection(editor: Editor) {
    if (!editor.selection) return false

    // Get all text nodes in the selection
    const texts = Array.from(
      Editor.nodes(editor, {
        at: editor.selection,
        match: (n) => Text.isText(n),
      })
    )

    // Check if any text node has an annotation
    return texts.some(([node]) => {
      return Text.isText(node) && node.annotated !== undefined
    })
  },

  addAnnotation(editor: Editor, data: AnnotationData) {
    Editor.addMark(editor, 'annotated', data)
  },

  removeAnnotation(editor: Editor) {
    Editor.removeMark(editor, 'annotated')
  },
}

// Error modal component
const ErrorModal = ({ message, onClose }: { message: string, onClose: () => void }) => {
  return (
    <div className="modal-overlay">
      <div className="modal-content modal-content-error">
        <h3>Error</h3>
        <p className="error-message">{message}</p>
        <div className="modal-actions">
          <button
            type="button"
            onClick={onClose}
            className="modal-button modal-button-submit"
          >
            OK
          </button>
        </div>
      </div>
    </div>
  )
}

// Modal component for annotation input
const AnnotationModal = ({ onClose, onSubmit }: { onClose: () => void, onSubmit: (data: AnnotationData) => void }) => {
  const [note, setNote] = useState('')

  const handleSubmit = () => {
    if (note.trim()) {
      onSubmit({ note: note.trim() })
      onClose()
    }
  }

  return (
    <div className="modal-overlay">
      <div className="modal-content">
        <h3>Add Annotation</h3>
        <div className="modal-field">
          <label className="modal-label">
            Note:
          </label>
          <textarea
            autoFocus
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Enter your annotation note..."
            className="modal-textarea"
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                handleSubmit()
              }
            }}
          />
        </div>
        <div className="modal-actions">
          <button
            type="button"
            onClick={onClose}
            className="modal-button modal-button-cancel"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            className="modal-button modal-button-submit"
          >
            Add Annotation
          </button>
        </div>
      </div>
    </div>
  )
}

// Toolbar component with annotation button
const Toolbar = () => {
  const editor = useSlate()
  const [showModal, setShowModal] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const hasSelection = CustomEditor.hasSelection(editor)
  const hasAnnotationInSelection = CustomEditor.hasAnnotationInSelection(editor)

  const handleAnnotationClick = () => {
    // Prevent overlapping annotations
    if (hasAnnotationInSelection) {
      setErrorMessage('The selection contains already annotated text. Remove the existing annotation first.')
      return
    }

    if (!hasSelection) {
      setErrorMessage('Please select some text to annotate.')
      return
    }

    setShowModal(true)
  }

  const handleAnnotationSubmit = (data: AnnotationData) => {
    CustomEditor.addAnnotation(editor, data)
  }

  const handleRemoveAnnotation = () => {
    if (!hasSelection) {
      setErrorMessage('Please select some text to remove annotation from.')
      return
    }

    if (!hasAnnotationInSelection) {
      setErrorMessage('The selection does not contain any annotated text.')
      return
    }

    CustomEditor.removeAnnotation(editor)
  }

  return (
    <>
      {errorMessage && (
        <ErrorModal
          message={errorMessage}
          onClose={() => setErrorMessage(null)}
        />
      )}
      {showModal && (
        <AnnotationModal
          onClose={() => setShowModal(false)}
          onSubmit={handleAnnotationSubmit}
        />
      )}
      <div className="toolbar">
        <button
          type="button"
          onClick={handleAnnotationClick}
          className={`toolbar-button ${hasAnnotationInSelection ? 'toolbar-button-active' : ''}`}
        >
          Annotate
        </button>
        {hasAnnotationInSelection && (
          <button
            type="button"
            onClick={handleRemoveAnnotation}
            className="toolbar-button"
          >
            Remove Annotation
          </button>
        )}
      </div>
    </>
  )
}

// Main editor component
export const AnnotationEditor = () => {
  const [editor] = useState(() => withReact(createEditor()))

  // Custom render for leaf nodes to handle bold formatting
  const renderLeaf = useCallback((props: any) => {
    return <Leaf {...props} />
  }, [])

  return (
    <Slate editor={editor} initialValue={initialValue}>
      <Toolbar />
      <Editable
        renderLeaf={renderLeaf}
        className="editor-editable"
      />
    </Slate>
  )
}

// Annotated text component - customize this for domain-specific functionality
const AnnotatedText = (props: any) => {
  const annotation = props.leaf.annotated as AnnotationData

  return (
    <span
      {...props.attributes}
      className="annotated-text"
      title={annotation.note}
    >
      {props.children}
    </span>
  )
}

// Leaf component to render text with formatting
const Leaf = (props: any) => {
  if (props.leaf.annotated) {
    return <AnnotatedText {...props} />
  }

  return (
    <span {...props.attributes}>
      {props.children}
    </span>
  )
}

const App = () => {
    return (
        <div className="app-container">
            <AnnotationEditor />
        </div>
    )
}

document.addEventListener('DOMContentLoaded', () => {
    const root = createRoot(document.getElementById('annotation-editor-root')!);
    root.render(<App />)
})