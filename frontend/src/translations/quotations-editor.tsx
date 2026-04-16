import React, { createContext, useCallback, useContext, useEffect, useReducer, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import type { QuotationPossiblyNew, QuotationsEditorElement, QuotationWithWordInfo, TextElement } from "./types";
import { ControlButton } from "../phonology-editor/controls";
import { Editable, Slate, withReact, type RenderElementProps, type RenderLeafProps } from "slate-react";
import { createEditor, Editor, Element, Transforms, type Descendant, Range } from "slate";
import { withHistory, HistoryEditor } from "slate-history";
import { ModalInner } from "../components/modal";
import { WordCombobox, type WordOption } from "../word-combobox";
import { AsyncSelect } from "../components/async-select";


type ControlModal = "AddQuotation" | "EditQuotation" | "RemoveQuotation";

interface Highlight {
  start: number;
  end: number;
}

const getTextOffset = (container: Node, targetNode: Node, targetOffset: number): number => {
  let offset = 0;
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let node: Node | null;
  while ((node = walker.nextNode()) !== null) {
    if (node === targetNode) return offset + targetOffset;
    offset += (node as Text).length;
  }
  return -1;
};

interface HighlightPickerProps {
  text: string;
  highlight: Highlight | null;
  onHighlightChange: (highlight: Highlight | null, text: string | null) => void;
}

const HighlightPicker = ({ text, highlight, onHighlightChange }: HighlightPickerProps) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const pendingRef = useRef<Highlight | null>(null);
  const [hasPending, setHasPending] = useState(false);

  useEffect(() => {
    const handleSelectionChange = () => {
      if (!containerRef.current) return;
      const selection = window.getSelection();
      if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
        pendingRef.current = null;
        setHasPending(false);
        return;
      }
      const range = selection.getRangeAt(0);
      if (!containerRef.current.contains(range.commonAncestorContainer)) {
        pendingRef.current = null;
        setHasPending(false);
      }
    };
    document.addEventListener('selectionchange', handleSelectionChange);
    return () => document.removeEventListener('selectionchange', handleSelectionChange);
  }, []);

  const captureSelection = () => {
    if (!containerRef.current) return;
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return;
    const range = selection.getRangeAt(0);
    if (!containerRef.current.contains(range.commonAncestorContainer)) return;
    if (range.startContainer.nodeType !== Node.TEXT_NODE || range.endContainer.nodeType !== Node.TEXT_NODE) return;
    const start = getTextOffset(containerRef.current, range.startContainer, range.startOffset);
    const end = getTextOffset(containerRef.current, range.endContainer, range.endOffset);
    if (start >= 0 && end >= 0 && start < end) {
      pendingRef.current = { start, end };
      setHasPending(true);
    }
  };

  const confirm = () => {
    const pending = pendingRef.current;
    if (pending) {
      onHighlightChange(pending, text.slice(pending.start, pending.end));
      pendingRef.current = null;
      setHasPending(false);
    }
  };

  const parts = highlight
    ? [
        { text: text.slice(0, highlight.start), highlighted: false },
        { text: text.slice(highlight.start, highlight.end), highlighted: true },
        { text: text.slice(highlight.end), highlighted: false },
      ]
    : [{ text, highlighted: false }];

  return (
    <div className="highlight-picker-wrapper">
      <div className="highlight-picker" ref={containerRef} onMouseUp={captureSelection} onTouchEnd={captureSelection}>
        {parts.map((part, i) =>
          part.highlighted
            ? <mark key={i}>{part.text}</mark>
            : <span key={i}>{part.text}</span>
        )}
      </div>
      <div className="highlight-picker-actions">
        <button type="button" className="normal secondary" disabled={!highlight} onClick={() => onHighlightChange(null, null)}>clear</button>
        <button type="button" className="normal primary" disabled={!hasPending} onClick={confirm}>confirm</button>
      </div>
    </div>
  );
};

interface EditorState {
  pendingModal: ControlModal | null;
  selection: Range | null;
  editingQuotation: QuotationPossiblyNew | null;
  selectedQuotation: QuotationPossiblyNew | null;
  selectionEdge: "before" | "after" | null;
  removingQuotation: QuotationPossiblyNew | null;
}

type Action =
  | { type: "OpenModal"; modal: "AddQuotation"; selection: Range | null }
  | { type: "OpenEditModal"; quotation: QuotationPossiblyNew }
  | { type: "OpenRemoveModal"; quotation: QuotationPossiblyNew }
  | { type: "ClearPendingModal" }
  | { type: "SelectQuotation"; quotation: QuotationPossiblyNew | null; edge?: "before" | "after" }

const editorInitialState: EditorState = { pendingModal: null, selection: null, editingQuotation: null, selectedQuotation: null, selectionEdge: "before", removingQuotation: null };

const applyAction = (state: EditorState, action: Action): EditorState => {
  switch (action.type) {
    case "OpenModal":
      return { ...state, pendingModal: action.modal, selection: action.selection };
    case "OpenEditModal":
      return { ...state, pendingModal: "EditQuotation", editingQuotation: action.quotation };
    case "OpenRemoveModal":
      return { ...state, pendingModal: "RemoveQuotation", removingQuotation: action.quotation };
    case "ClearPendingModal":
      return { ...state, pendingModal: null, editingQuotation: null, removingQuotation: null };
    case "SelectQuotation":
      return { ...state, selectedQuotation: action.quotation, selectionEdge: action.edge ?? null };
    default:
      return state;
  }
};

const EditorContext = createContext<[EditorState, React.Dispatch<Action>]>(null!);
const useEditor = () => useContext(EditorContext);

interface QuotationsEditorProps {
  quotations: QuotationWithWordInfo[];
  text: string;
  languageCode: string;
  error?: string[];
}

interface EditorBaseProps {
  editor: Editor;
  languageCode: string;
}

interface ControlsProps extends EditorBaseProps {
  quotationsCount: number;
  onSelectPrev: () => void;
  onSelectNext: () => void;
  onRemoveQuotation: () => void;
}

interface DefinitionOption {
  id: string;
  definition: string;
  context: string;
  position: number;
}

interface AddQuotationModalProps extends EditorBaseProps {
  open: boolean;
  setOpen: (open: boolean) => void;
  selection: Range | null;
}

const addQuotation = (selectedWord: WordOption | null, selectedDefinition: DefinitionOption | null, selection: Range | null, editor: Editor, highlight: Highlight | null, notes: string) => {
  if (!selectedWord || !selectedDefinition || !selection) {
    return;
  }

  const { id: definition_id, definition } = selectedDefinition;
  const { slug: word_slug, lemma: word_lemma, word } = selectedWord;

  const span_start = Range.start(selection).offset;
  const span_end = Range.end(selection).offset;

  const quotationInfo: QuotationPossiblyNew = {
    definition_id,
    definition_text: definition,
    highlight_start: highlight?.start ?? null,
    highlight_end: highlight?.end ?? null,
    notes,
    word_slug,
    word_lemma,
    word,
    span_start,
    span_end,
  };

  Editor.addMark(editor, 'quotation', quotationInfo);
}

const loadDefinitions = (word: WordOption, languageCode: string, setDefinitionsError: (error: string) => void) => async (input: string): Promise<DefinitionOption[]> => {
  try {
    const response = await fetch(`/api/languages/${languageCode}/words/${word.slug}/${word.lemma}/definitions`);
    if (!response.ok) {
      console.error("Failed to load definitions:", response.statusText);
      setDefinitionsError("Failed to load definitions");
      return [];
    }
    const data: { items: DefinitionOption[] } = await response.json();
    const definitions = data.items;
    setDefinitionsError("");
    return definitions.filter(def => def.definition.toLowerCase().includes(input.toLowerCase()));
  } catch(e) {
    console.error("Error loading definitions:", e);
    setDefinitionsError("Error loading definitions");
    return [];
  }
}

const formatDefinitionLabel = (option: DefinitionOption) => {
  return (
    <div className="definition-option">
      {option.context ? (
        <div className="context-container">(<span className="context">{option.context}</span>)</div>
      ) : null}
      <div className="definition">{option.definition}</div>
    </div>
  )
}

const AddQuotationModal = ({ editor, open, setOpen, languageCode, selection }: AddQuotationModalProps) => {
  const [selectedWord, setSelectedWord] = useState<WordOption | null>(null);
  const [definitionsError, setDefinitionsError] = useState<string>("");
  const [selectedDefinition, setSelectedDefinition] = useState<DefinitionOption | null>(null);
  const [highlight, setHighlight] = useState<Highlight | null>(null);
  const [highlightText, setHighlightText] = useState<string | null>(null);
  const [notes, setNotes] = useState<string>("");

  useEffect(() => {
    if (open) {
      setHighlight(null);
      setHighlightText(null);
      setNotes("");
    }
  }, [open]);

  const quotationText = selection ? Editor.string(editor, selection) : '';

  const handleHighlightChange = (h: Highlight | null, text: string | null) => {
    setHighlight(h);
    setHighlightText(text);
  };

  return <ModalInner open={open} close={() => setOpen(false)} title="Add quotation" contents={(close) => (
    <form className="default" onSubmit={(e) => { e.preventDefault(); addQuotation(selectedWord, selectedDefinition, selection, editor, highlight, notes); close(); }}>
      {quotationText && (
        <section>
          <label>Highlight</label>
          <p className="hint">select the part of the text which represents the word you want to link to</p>
          <HighlightPicker text={quotationText} highlight={highlight} onHighlightChange={handleHighlightChange} />
        </section>
      )}

      <section>
        <label htmlFor="add-quotation-word">Word</label>
        <WordCombobox
          key={highlightText ?? ''}
          inputId="add-quotation-word"
          inputName="add-quotation-word"
          languageFilter={languageCode}
          onChange={setSelectedWord}
          initialOption={selectedWord ? selectedWord : undefined}
          initialSearch={selectedWord ? undefined : highlightText ?? undefined}
        />
      </section>

      {selectedWord ?
          <section>
            <label htmlFor="add-quotation-definition">Definition</label>
            {definitionsError && <div className="error">{definitionsError}</div>}
            <AsyncSelect<DefinitionOption, false>
              inputId="add-quotation-definition"
              cacheOptions
              defaultOptions
              loadOptions={loadDefinitions(selectedWord, languageCode, setDefinitionsError)}
              formatOptionLabel={formatDefinitionLabel}
              onChange={setSelectedDefinition}
            />
            <span className="hint">
              to add another definition, <a href={`/languages/${languageCode}/words/${selectedWord.slug}/${selectedWord.lemma}/edit`}>edit this word</a>
            </span>
          </section>
          : ""
      }

      <section>
        <label htmlFor="add-quotation-notes">Notes</label>
        <input id="add-quotation-notes" type="text" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="optional notes" />
      </section>

      <div className="button-row">
        <button className="secondary normal" type="button" onClick={close}>Cancel</button>
        <button className="primary normal" type="button" onClick={() => { addQuotation(selectedWord, selectedDefinition, selection, editor, highlight, notes); close(); }}>Add</button>
      </div>
    </form>
  )} />
}

interface EditQuotationModalProps extends EditorBaseProps {
  open: boolean;
  setOpen: (open: boolean) => void;
  quotation: QuotationPossiblyNew;
  onSave: (updatedQuotation: QuotationPossiblyNew) => void;
}

const EditQuotationModal = ({ editor, open, setOpen, quotation, onSave, languageCode }: EditQuotationModalProps) => {
  const [selectedWord, setSelectedWord] = useState<WordOption | null>(() => ({
    id: '',
    word: quotation.word,
    slug: quotation.word_slug,
    lemma: quotation.word_lemma,
    bookmark: '',
    language_code: languageCode,
    language_name: languageCode,
    label: quotation.word,
    value: quotation.word_slug,
  }));

  const [definitionsError, setDefinitionsError] = useState<string>("");

  const [selectedDefinition, setSelectedDefinition] = useState<DefinitionOption | null>(() => ({
    id: quotation.definition_id,
    definition: quotation.definition_text,
    context: '',
    position: 0,
  }));

  const [highlight, setHighlight] = useState<Highlight | null>(() =>
    quotation.highlight_start !== null && quotation.highlight_end !== null
      ? { start: quotation.highlight_start, end: quotation.highlight_end }
      : null
  );
  const [highlightText, setHighlightText] = useState<string | null>(null);
  const [notes, setNotes] = useState<string>(quotation.notes ?? "");

  const quotationText = Array.from(
    Editor.nodes<TextElement>(editor, {
      at: [],
      match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && (n as TextElement).quotation === quotation,
    })
  ).map(([node]) => (node as TextElement).text).join('');

  const handleHighlightChange = (h: Highlight | null, text: string | null) => {
    setHighlight(h);
    setHighlightText(text);
  };

  const save = () => {
      if (!selectedWord || !selectedDefinition) {
        return;
      }

      const { id: definition_id, definition } = selectedDefinition;
      const { slug: word_slug, lemma: word_lemma, word } = selectedWord;

      const quotationInfo: QuotationPossiblyNew = {
        ...quotation,
        definition_id,
        definition_text: definition,
        word_slug,
        word_lemma,
        word,
        highlight_start: highlight?.start ?? null,
        highlight_end: highlight?.end ?? null,
        notes,
      };

      onSave(quotationInfo);
      setOpen(false);
  }

  return <ModalInner open={open} close={() => setOpen(false)} title="Edit quotation" contents={(close) => (
    <form className="default" onSubmit={(e) => { e.preventDefault(); save(); close(); }}>
      {quotationText && (
        <section>
          <label>Highlight</label>
          <p className="hint">Select part of the text to highlight it and pre-fill the word search.</p>
          <HighlightPicker text={quotationText} highlight={highlight} onHighlightChange={handleHighlightChange} />
        </section>
      )}

      <section>
        <label htmlFor="edit-quotation-word">Word</label>
        <WordCombobox
          key={highlightText ?? ''}
          inputId="edit-quotation-word"
          inputName="edit-quotation-word"
          languageFilter={languageCode}
          initialOption={highlightText ? undefined : selectedWord}
          onChange={setSelectedWord}
          initialSearch={highlightText ?? undefined}
        />
      </section>

      {selectedWord ?
          <section>
            <label htmlFor="edit-quotation-definition">Definition</label>
            {definitionsError && <div className="error">{definitionsError}</div>}
            <AsyncSelect<DefinitionOption, false>
              inputId="edit-quotation-definition"
              cacheOptions
              defaultOptions
              loadOptions={loadDefinitions(selectedWord, languageCode, setDefinitionsError)}
              formatOptionLabel={formatDefinitionLabel}
              value={selectedDefinition}
              onChange={setSelectedDefinition}
            />
            <span className="hint">
              to add another definition, <a href={`/languages/${languageCode}/words/${selectedWord.slug}/${selectedWord.lemma}/edit`}>edit this word</a>
            </span>
          </section>
          : ""
      }

      <section>
        <label htmlFor="edit-quotation-notes">Notes</label>
        <input id="edit-quotation-notes" type="text" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="optional notes" />
      </section>

      <div className="button-row">
        <button className="secondary normal" type="button" onClick={close}>Cancel</button>
        <button className="primary normal" type="button" onClick={() => { save(); }}>Save</button>
      </div>
    </form>
  )} />
}

interface RemoveQuotationModalProps {
  open: boolean;
  setOpen: (open: boolean) => void;
  quotation: QuotationPossiblyNew;
  onConfirm: () => void;
}

const RemoveQuotationModal = ({ open, setOpen, quotation, onConfirm }: RemoveQuotationModalProps) => {
  return <ModalInner open={open} close={() => setOpen(false)} title="Remove quotation" contents={(close) => (
    <div>
      <p>Remove the quotation for <strong>{quotation.word}</strong>?</p>
      <p className="hint">{quotation.definition_text}</p>
      <div className="button-row">
        <button className="secondary normal" type="button" onClick={close}>Cancel</button>
        <button className="danger normal" type="button" onClick={() => { onConfirm(); close(); }}>Remove</button>
      </div>
    </div>
  )} />
}

const HelpIcon = () => (
  <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">
    {/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}
    <path fill="currentColor" d="M11.95 18q.525 0 .888-.363t.362-.887t-.362-.888t-.888-.362t-.887.363t-.363.887t.363.888t.887.362m.05 4q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22m0-2q3.35 0 5.675-2.325T20 12t-2.325-5.675T12 4T6.325 6.325T4 12t2.325 5.675T12 20m.1-12.3q.625 0 1.088.4t.462 1q0 .55-.337.975t-.763.8q-.575.5-1.012 1.1t-.438 1.35q0 .35.263.588t.612.237q.375 0 .638-.25t.337-.625q.1-.525.45-.937t.75-.788q.575-.55.988-1.2t.412-1.45q0-1.275-1.037-2.087T12.1 6q-.95 0-1.812.4T8.975 7.625q-.175.3-.112.638t.337.512q.35.2.725.125t.625-.425q.275-.375.688-.575t.862-.2" />
  </svg>
);

const Keybind = ({ keys, description }: { keys: string[]; description: string }) => (
  <>
    <dt>{keys.map((key, index) => <kbd key={key + index}>{key}</kbd>)}</dt>
    <dd>{description}</dd>
  </>
);

const QuotationsEditorControls = ({ editor, languageCode, quotationsCount, onSelectPrev, onSelectNext, onRemoveQuotation }: ControlsProps) => {
  const [editorState, dispatch] = useEditor();
  const [helpOpen, setHelpOpen] = useState(false);

  const openAddQuotation = () => {
    dispatch({ type: "OpenModal", modal: "AddQuotation", selection: editor.selection });
  }

  const openEditQuotation = () => {
    if (editorState.selectedQuotation) {
      dispatch({ type: "OpenEditModal", quotation: editorState.selectedQuotation });
    }
  }

  const atLeftEdge = quotationsCount === 0 || (editorState.selectedQuotation === null && editorState.selectionEdge === "before");
  const atRightEdge = quotationsCount === 0 || (editorState.selectedQuotation === null && editorState.selectionEdge === "after");
  const selectionCoversQuotation = editor.selection !== null && !Range.isCollapsed(editor.selection) &&
    Array.from(Editor.nodes<TextElement>(editor, {
      at: editor.selection,
      match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && !!(n as TextElement).quotation,
    })).length > 0;
  const canRemove = editorState.selectedQuotation !== null || selectionCoversQuotation;

  const canUndo = (editor as HistoryEditor).history.undos.length > 0;
  const canRedo = (editor as HistoryEditor).history.redos.length > 0;

  return (
    <>
    <div className="controls-container">
        <div className="controls editor-controls">
          <span className="controls-header">Editor</span>
          <ControlButton onClick={() => HistoryEditor.undo(editor)} title="Undo" enabled={canUndo}>
            <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M8 19q-.425 0-.712-.288T7 18t.288-.712T8 17h6.1q1.575 0 2.738-1T18 13.5T16.838 11T14.1 10H7.8l1.9 1.9q.275.275.275.7t-.275.7t-.7.275t-.7-.275L4.7 9.7q-.15-.15-.213-.325T4.426 9t.063-.375T4.7 8.3l3.6-3.6q.275-.275.7-.275t.7.275t.275.7t-.275.7L7.8 8h6.3q2.425 0 4.163 1.575T20 13.5t-1.737 3.925T14.1 19z" /></svg>
          </ControlButton>
          <ControlButton onClick={() => HistoryEditor.redo(editor)} title="Redo" enabled={canRedo}>
            <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">{/* Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE */}<path fill="currentColor" d="M9.9 19q-2.425 0-4.163-1.575T4 13.5t1.738-3.925T9.9 8h6.3l-2.6-2.6L15 4l5 5l-5 5l-1.4-1.4l2.6-2.6H9.9q-1.575 0-2.738 1T6 13.5T7.163 16T9.9 17H17v2z" /></svg>
          </ControlButton>
          <ControlButton onClick={() => setHelpOpen(true)} title="Editor help" enabled={true}>
            <HelpIcon />
          </ControlButton>
        </div>
        <div className="controls quotation-controls">
        <span className="controls-header">Quotations</span>
        <ControlButton onClick={openAddQuotation} title="Add at selection" enabled={editor.selection !== null}>
            <svg className="icon" aria-hidden><use href="#icon-plus" /></svg>
        </ControlButton>
        <ControlButton onClick={onSelectPrev} title="Previous quotation" enabled={!atLeftEdge}>
            <svg className="icon" aria-hidden><use href="#icon-chevron-left" /></svg>
        </ControlButton>
        <ControlButton onClick={onSelectNext} title="Next quotation" enabled={!atRightEdge}>
            <svg className="icon" aria-hidden><use href="#icon-chevron-right" /></svg>
        </ControlButton>
        <ControlButton onClick={openEditQuotation} title="Edit quotation" enabled={editorState.selectedQuotation != null}>
            <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="currentColor" d="M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z" /></svg>
        </ControlButton>
        <ControlButton onClick={onRemoveQuotation} title="Remove quotation" enabled={canRemove}>
            <svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="currentColor" d="M19 6.41L17.59 5L12 10.59L6.41 5L5 6.41L10.59 12L5 17.59L6.41 19L12 13.41L17.59 19L19 17.59L13.41 12z"/></svg>
        </ControlButton>
      </div>
    </div>
    <ModalInner
      open={helpOpen}
      close={() => setHelpOpen(false)}
      title="editor help"
      contents={(close) => (
        <>
          <div className="help-content">
            <section>
              <h3>Undo and redo</h3>
              <dl className="keybind-list">
                <Keybind keys={["Ctrl", "z"]} description="Undo" />
                <Keybind keys={["Ctrl", "Shift", "z"]} description="Redo" />
                <Keybind keys={["Ctrl", "y"]} description="Redo" />
              </dl>
            </section>
            <section>
              <h3>Quotations</h3>
              <dl className="keybind-list">
                <Keybind keys={["Ctrl", "k"]} description="Add quotation at selection" />
              </dl>
            </section>
          </div>
          <div className="button-row">
            <button type="button" className="normal" onClick={close}>Close</button>
          </div>
        </>
      )}
    />
    </>
  )
}

const Leaf = (props: RenderLeafProps) => {
  const [editorState, dispatch] = useEditor();

  const handleClick = () => {
    if (props.leaf.quotation) {
      dispatch({ type: "SelectQuotation", quotation: props.leaf.quotation });
    }
  };

  const isSelected = props.leaf.quotation != null && props.leaf.quotation === editorState.selectedQuotation;

  let className: string | undefined;
  let inner;
  if (props.leaf.quotation) {
    const parts = ['quotation'];
    if (isSelected) parts.push('selected');
    className = parts.join(' ');

    if (props.leaf.quotation.highlight_start !== null && props.leaf.quotation.highlight_end !== null) {
      const { highlight_start: hs, highlight_end: he } = props.leaf.quotation;
      const text = props.leaf.text;
      inner = (
        <>
          {text.slice(0, hs)}
          <span className="highlight">{text.slice(hs, he)}</span>
          {text.slice(he)}
        </>
      );
    } else {
      inner = props.children;
    }
  } else {
    inner = props.children;
  }

  return (
    <span
      {...props.attributes}
      className={className}
      onClick={props.leaf.quotation ? handleClick : undefined}
    >
      {inner}
    </span>
  )
}

const initialValue = (props: QuotationsEditorProps): QuotationsEditorElement[] => {
  const parsed = props.quotations.reduce((acc, quotation) => {
    const beforeText = props.text.slice(acc[1], quotation.span_start);
    const quotationText = props.text.slice(quotation.span_start, quotation.span_end);
    const cs =  [
      ...acc[0],
      { type: 'text', text: beforeText },
      { type: 'text', text: quotationText, quotation }
    ];
    return [cs, quotation.span_end] as [TextElement[], number];
  }, [[] as TextElement[], 0] as [TextElement[], number]);

  const children = parsed[0];

  // need to add the remaining text after the last quotation
  const remainingText = props.text.slice(parsed[1]);
  if (remainingText) {
    children.push({ type: 'text', text: remainingText });
  }

  return [
    {
      type: 'paragraph',
      children
    },
  ]
}

const getQuotations = (editor: Editor): QuotationPossiblyNew[] => {
  const inner = (children: Descendant[]): [QuotationPossiblyNew[], number] => 
    children.reduce(([quotations, offset], node) => {
      if (node.type === 'paragraph') {
        const [childQuotations, childLength] = inner(node.children);
        return [[...quotations, ...childQuotations], offset + childLength];
      }

      let newQuotations = quotations;
      if(node.quotation) {
        newQuotations = [...quotations, node.quotation];
      }
      return [newQuotations, offset + node.text.length];
    }, [[] as QuotationPossiblyNew[], 0] as [QuotationPossiblyNew[], number]);

  const [quotations] = inner(editor.children);
  return quotations;
}

const getTextValue = (editor: Editor): string => {
  const getText = (children: Descendant[]): string =>
    children.reduce((text, node) => {
      if (node.type === 'paragraph') {
        return text + getText(node.children) + '\n\n';
      }
      return text + node.text;
    }, '');

  return getText(editor.children);
}

const QuotationsEditor = (props: QuotationsEditorProps) => {
  const [editor] = useState(() => withHistory(withReact(createEditor())));
  const [editorState, dispatch] = useReducer(applyAction, editorInitialState);
  const initial = initialValue(props);
  const [quotations, sq] = useState<QuotationPossiblyNew[]>(props.quotations);
  const [text, setText] = useState(props.text);

  const renderElement = useCallback(({ attributes, children }: RenderElementProps) => {
    return <span {...attributes}>{children}</span>
  }, [])

  const renderLeaf = useCallback((props: RenderLeafProps) => {
    return <Leaf {...props} />
  }, [])

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.ctrlKey || event.metaKey) {
      const key = event.key.toLowerCase();
      if (!event.shiftKey && key === 'z') {
        event.preventDefault();
        HistoryEditor.undo(editor);
        return;
      }
      if ((event.shiftKey && key === 'z') || key === 'y') {
        event.preventDefault();
        HistoryEditor.redo(editor);
        return;
      }
      if (!event.shiftKey && key === 'k') {
        if (editor.selection && !Range.isCollapsed(editor.selection)) {
          event.preventDefault();
          dispatch({ type: "OpenModal", modal: "AddQuotation", selection: editor.selection });
          return;
        }
      }
    }
  }

  const onSlateChange = () => {
    sq(getQuotations(editor));
    setText(getTextValue(editor));
  }

  const handleEditSave = (oldQuotation: QuotationPossiblyNew, updatedQuotation: QuotationPossiblyNew) => {
    const nodeEntries = Array.from(
      Editor.nodes(editor, {
        at: [],
        match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && (n as TextElement).quotation === oldQuotation,
      })
    );
    for (const [, path] of nodeEntries) {
      Transforms.setNodes<TextElement>(editor, { quotation: updatedQuotation }, { at: path });
    }
    dispatch({ type: "SelectQuotation", quotation: updatedQuotation });
  }

  const handleSelectNext = () => {
    const nodes = getQuotations(editor);
    if (nodes.length === 0) return;
    if (editorState.selectedQuotation !== null) {
      const currentIndex = nodes.indexOf(editorState.selectedQuotation);
      if (currentIndex === -1 || currentIndex >= nodes.length - 1) {
        dispatch({ type: "SelectQuotation", quotation: null, edge: "after" });
      } else {
        const next = nodes[currentIndex + 1];
        if (next) dispatch({ type: "SelectQuotation", quotation: next });
      }
    } else {
      const first = nodes[0];
      if (first) dispatch({ type: "SelectQuotation", quotation: first });
    }
  };

  const handleRemoveQuotation = () => {
    const selection = editor.selection;

    if (selection && !Range.isCollapsed(selection)) {
      const nodesInSelection = Array.from(
        Editor.nodes<TextElement>(editor, {
          at: selection,
          match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && !!(n as TextElement).quotation,
        })
      );

      if (nodesInSelection.length === 0) return;

      const firstNode = nodesInSelection[0];
      if (!firstNode) return;
      const quotation = (firstNode[0] as TextElement).quotation!;

      const allQuotationNodes = Array.from(
        Editor.nodes<TextElement>(editor, {
          at: [],
          match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && (n as TextElement).quotation === quotation,
        })
      );

      const wouldRemoveEntire = allQuotationNodes.every(([node, path]) => {
        const nodeStart = { path, offset: 0 };
        const nodeEnd = { path, offset: (node as TextElement).text.length };
        return Range.includes(selection, nodeStart) && Range.includes(selection, nodeEnd);
      });

      if (wouldRemoveEntire) {
        dispatch({ type: "OpenRemoveModal", quotation });
      } else {
        Editor.removeMark(editor, 'quotation');
      }
    } else if (editorState.selectedQuotation) {
      dispatch({ type: "OpenRemoveModal", quotation: editorState.selectedQuotation });
    }
  };

  const handleConfirmRemove = (quotation: QuotationPossiblyNew) => {
    const nodeEntries = Array.from(
      Editor.nodes<TextElement>(editor, {
        at: [],
        match: (n) => !Editor.isEditor(n) && !Element.isElement(n) && (n as TextElement).quotation === quotation,
      })
    );
    for (const [, path] of nodeEntries) {
      Transforms.unsetNodes(editor, 'quotation', { at: path });
    }
    if (editorState.selectedQuotation === quotation) {
      dispatch({ type: "SelectQuotation", quotation: null });
    }
  };

  const handleSelectPrev = () => {
    const nodes = getQuotations(editor);
    if (nodes.length === 0) return;
    if (editorState.selectedQuotation !== null) {
      const currentIndex = nodes.indexOf(editorState.selectedQuotation);
      if (currentIndex <= 0) {
        dispatch({ type: "SelectQuotation", quotation: null, edge: "before" });
      } else {
        const prev = nodes[currentIndex - 1];
        if (prev) dispatch({ type: "SelectQuotation", quotation: prev });
      }
    } else {
      const last = nodes[nodes.length - 1];
      if (last) dispatch({ type: "SelectQuotation", quotation: last });
    }
  };

  return (
    <EditorContext.Provider value={[editorState, dispatch]}>
      <section className="quotations-editor">
          <label id="quotation-editor-label" className="editor-label">Translated text</label>
        {props.error ? <ul className="field-errors">
          {props.error.map((err, index) => <li key={index}>{err}</li>)}
        </ul> : ""}
        <QuotationsEditorControls editor={editor} languageCode={props.languageCode} quotationsCount={quotations.length} onSelectPrev={handleSelectPrev} onSelectNext={handleSelectNext} onRemoveQuotation={handleRemoveQuotation} />
        <Slate
          editor={editor}
          initialValue={initial}
          onChange={onSlateChange}
        >
          <Editable
            renderElement={renderElement}
            renderLeaf={renderLeaf}
            onKeyDown={onKeyDown}
            aria-labelledby="quotation-editor-label"
            className="quotation-editor"
          />
        </Slate>
        <input id="translated_text" name="translated_text" required hidden readOnly value={text} />
        <input id="quotations" name="quotations" hidden readOnly value={JSON.stringify(quotations)} />
        <AddQuotationModal
          editor={editor}
          open={editorState.pendingModal === "AddQuotation"}
          selection={editorState.selection}
          setOpen={(open) => { if (!open) dispatch({ type: "ClearPendingModal" }); }}
          languageCode={props.languageCode}
        />
        {editorState.editingQuotation && (
          <EditQuotationModal
            editor={editor}
            open={editorState.pendingModal === "EditQuotation"}
            setOpen={(open) => { if (!open) dispatch({ type: "ClearPendingModal" }); }}
            languageCode={props.languageCode}
            quotation={editorState.editingQuotation}
            onSave={(updated) => handleEditSave(editorState.editingQuotation!, updated)}
          />
        )}
        {editorState.removingQuotation && (
          <RemoveQuotationModal
            open={editorState.pendingModal === "RemoveQuotation"}
            setOpen={(open) => { if (!open) dispatch({ type: "ClearPendingModal" }); }}
            quotation={editorState.removingQuotation}
            onConfirm={() => handleConfirmRemove(editorState.removingQuotation!)}
          />
        )}
      </section>
    </EditorContext.Provider>
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
