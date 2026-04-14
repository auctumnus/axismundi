import React, { useState, useReducer, useEffect } from "react";
import * as ReactDOM from "react-dom/client";
import { AnimatePresence, motion } from "motion/react";
import { ModalInner } from "./components/modal";
import { Tooltip } from "./components/tooltip";

interface DefinitionItem {
  key: string;
  id: string; // empty string for new items
  definition: string;
  context: string;
}

interface SnapState {
  items: DefinitionItem[];
  selectedKey: string | null;
}

interface EditorState extends SnapState {
  undoStack: SnapState[];
  redoStack: SnapState[];
}

type Action =
  | { type: "Select"; key: string | null }
  | { type: "Update"; key: string; definition: string; context: string }
  | { type: "Add" }
  | { type: "Remove" }
  | { type: "MoveUp" }
  | { type: "MoveDown" }
  | { type: "Undo" }
  | { type: "Redo" };

const MAX_DEFINITIONS = 10;

function generateKey(): string {
  return Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);
}

function snap(state: EditorState): SnapState {
  return { items: state.items, selectedKey: state.selectedKey };
}

function reducer(state: EditorState, action: Action): EditorState {
  switch (action.type) {
    case "Select":
      return { ...state, selectedKey: action.key };

    case "Update": {
      const items = state.items.map((item) =>
        item.key === action.key
          ? { ...item, definition: action.definition, context: action.context }
          : item
      );
      return withUndo(state, { ...state, items });
    }

    case "Add": {
      if (state.items.length >= MAX_DEFINITIONS) return state;
      const newItem: DefinitionItem = { key: generateKey(), id: "", definition: "", context: "" };
      let items: DefinitionItem[];
      if (state.selectedKey === null) {
        items = [...state.items, newItem];
      } else {
        const idx = state.items.findIndex((item) => item.key === state.selectedKey);
        items = [...state.items];
        items.splice(idx + 1, 0, newItem);
      }
      return withUndo(state, { ...state, items, selectedKey: newItem.key });
    }

    case "Remove": {
      if (state.selectedKey === null || state.items.length <= 1) return state;
      const idx = state.items.findIndex((item) => item.key === state.selectedKey);
      const items = state.items.filter((item) => item.key !== state.selectedKey);
      const newSelectedKey = items[idx]?.key ?? items[idx - 1]?.key ?? null;
      return withUndo(state, { ...state, items, selectedKey: newSelectedKey });
    }

    case "MoveUp": {
      if (state.selectedKey === null) return state;
      const idx = state.items.findIndex((item) => item.key === state.selectedKey);
      if (idx <= 0) return state;
      const items = [...state.items];
      [items[idx - 1], items[idx]] = [items[idx]!, items[idx - 1]!];
      return withUndo(state, { ...state, items });
    }

    case "MoveDown": {
      if (state.selectedKey === null) return state;
      const idx = state.items.findIndex((item) => item.key === state.selectedKey);
      if (idx >= state.items.length - 1) return state;
      const items = [...state.items];
      [items[idx], items[idx + 1]] = [items[idx + 1]!, items[idx]!];
      return withUndo(state, { ...state, items });
    }

    case "Undo": {
      if (state.undoStack.length === 0) return state;
      const prev = state.undoStack[state.undoStack.length - 1]!;
      return {
        ...prev,
        undoStack: state.undoStack.slice(0, -1),
        redoStack: [...state.redoStack, snap(state)],
      };
    }

    case "Redo": {
      if (state.redoStack.length === 0) return state;
      const next = state.redoStack[state.redoStack.length - 1]!;
      return {
        ...next,
        undoStack: [...state.undoStack, snap(state)],
        redoStack: state.redoStack.slice(0, -1),
      };
    }

    default:
      return state;
  }
}

function withUndo(prev: EditorState, next: EditorState): EditorState {
  return {
    ...next,
    undoStack: [...prev.undoStack, snap(prev)],
    redoStack: [],
  };
}

interface DefinitionsEditorProps {
  initialItems: DefinitionItem[];
  isEdit: boolean;
}

function DefinitionRow({
  item,
  isEdit,
  isSelected,
  onSelect,
  onChange,
}: {
  item: DefinitionItem;
  isEdit: boolean;
  isSelected: boolean;
  onSelect: () => void;
  onChange: (def: string, ctx: string) => void;
}) {
  const [localDef, setLocalDef] = useState(item.definition);
  const [localCtx, setLocalCtx] = useState(item.context);

  // Sync from parent when undo/redo changes values externally
  useEffect(() => { setLocalDef(item.definition); }, [item.definition]);
  useEffect(() => { setLocalCtx(item.context); }, [item.context]);

  const handleBlur = () => {
    if (localDef !== item.definition || localCtx !== item.context) {
      onChange(localDef, localCtx);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect();
    }
  };

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.18 }}
      className={`definition-item${isSelected ? " selected" : ""}`}
      onClick={onSelect}
      onKeyDown={onKeyDown}
    >
      {isEdit && <input type="hidden" name="definition_ids[]" value={item.id} />}

      <input
        type="radio"
        className="definition-select"
        aria-label="select definition"
        checked={isSelected}
        onChange={() => {}}
        onClick={(e) => { e.stopPropagation(); onSelect(); }}
        onKeyDown={onKeyDown}
      />

      <input
        type="text"
        name="definitions[]"
        aria-label="Definition"
        required
        className="definition-input"
        value={localDef}
        onChange={(e) => setLocalDef(e.target.value)}
        onBlur={handleBlur}
        onClick={(e) => e.stopPropagation()}
      />
      <input
        type="text"
        name="contexts[]"
        aria-label="Context (optional)"
        value={localCtx}
        onChange={(e) => setLocalCtx(e.target.value)}
        onBlur={handleBlur}
        onClick={(e) => e.stopPropagation()}
      />
    </motion.div>
  );
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

function DefinitionsEditor({ initialItems, isEdit }: DefinitionsEditorProps) {
  const [state, dispatch] = useReducer(reducer, null, () => {
    const items =
      initialItems.length === 0
        ? [{ key: generateKey(), id: "", definition: "", context: "" }]
        : initialItems;
    return { items, selectedKey: null, undoStack: [], redoStack: [] };
  });

  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);

  const { items, selectedKey, undoStack, redoStack } = state;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!e.ctrlKey) return;
      const key = e.key.toLowerCase();
      if (!e.shiftKey && key === "z") {
        e.preventDefault();
        dispatch({ type: "Undo" });
      } else if ((e.shiftKey && key === "z") || key === "y") {
        e.preventDefault();
        dispatch({ type: "Redo" });
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        dispatch({ type: "MoveUp" });
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        dispatch({ type: "MoveDown" });
      } else if (e.key === "Enter") {
        e.preventDefault();
        dispatch({ type: "Add" });
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  const selectedItem = items.find((item) => item.key === selectedKey) ?? null;
  const selectedIndex = selectedItem ? items.indexOf(selectedItem) : -1;

  const handleRemoveClick = () => {
    if (!selectedItem) return;
    const hasContent = selectedItem.definition.trim() || selectedItem.context.trim();
    if (hasContent) {
      setConfirmDeleteOpen(true);
    } else {
      dispatch({ type: "Remove" });
    }
  };

  const confirmRemove = () => {
    setConfirmDeleteOpen(false);
    dispatch({ type: "Remove" });
  };

  return (
    <>
      <div className="definitions-input-header">
        <label>Definitions</label>
        <div className="definitions-controls">
          <div className="def-controls">
            <span className="controls-header">Rows</span>
            <Tooltip content="move up">
              <button
                type="button"
                className="control-button first"
                onClick={() => dispatch({ type: "MoveUp" })}
                disabled={selectedIndex <= 0}
                aria-label="move up"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-arrow-upward"></use>
                </svg>
              </button>
            </Tooltip>
            <Tooltip content="move down">
              <button
                type="button"
                className="control-button"
                onClick={() => dispatch({ type: "MoveDown" })}
                disabled={selectedIndex < 0 || selectedIndex >= items.length - 1}
                aria-label="move down"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-arrow-downward"></use>
                </svg>
              </button>
            </Tooltip>
            <Tooltip content="add definition">
              <button
                type="button"
                className="control-button"
                onClick={() => dispatch({ type: "Add" })}
                disabled={items.length >= MAX_DEFINITIONS}
                aria-label="add definition"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-plus"></use>
                </svg>
              </button>
            </Tooltip>
            <Tooltip content="remove selected">
              <button
                type="button"
                className="control-button"
                onClick={handleRemoveClick}
                disabled={!selectedKey || items.length <= 1}
                aria-label="remove definition"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-close-small"></use>
                </svg>
              </button>
            </Tooltip>
          </div>

          <div className="def-controls">
            <span className="controls-header">Editor</span>
            <Tooltip content="undo">
              <button
                type="button"
                className="control-button first"
                onClick={() => dispatch({ type: "Undo" })}
                disabled={undoStack.length === 0}
                aria-label="undo"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-undo"></use>
                </svg>
              </button>
            </Tooltip>
            <Tooltip content="redo">
              <button
                type="button"
                className="control-button"
                onClick={() => dispatch({ type: "Redo" })}
                disabled={redoStack.length === 0}
                aria-label="redo"
              >
                <svg className="icon" aria-hidden="true">
                  <use href="#icon-redo"></use>
                </svg>
              </button>
            </Tooltip>
            <Tooltip content="editor help">
              <button
                type="button"
                className="control-button"
                onClick={() => setHelpOpen(true)}
                aria-label="editor help"
              >
                <HelpIcon />
              </button>
            </Tooltip>
          </div>
        </div>
      </div>
      
      <div className="definitions-header">
        <span className="definitions-header-spacer" aria-hidden="true" />
        <label className="definitions-col-label">definition</label>
        <label className="definitions-col-label">context</label>
      </div>

      <div id="definitions-container">
        <AnimatePresence initial={false}>
          {items.map((item) => (
            <DefinitionRow
              key={item.key}
              item={item}
              isEdit={isEdit}
              isSelected={item.key === selectedKey}
              onSelect={() => dispatch({ type: "Select", key: item.key === selectedKey ? null : item.key })}
              onChange={(def, ctx) =>
                dispatch({ type: "Update", key: item.key, definition: def, context: ctx })
              }
            />
          ))}
        </AnimatePresence>
      </div>

      <ModalInner
        open={confirmDeleteOpen}
        close={() => setConfirmDeleteOpen(false)}
        title="remove definition"
        contents={(close) => (
          <>
            <p>this definition has content. are you sure you want to remove it?</p>
            <div className="button-row">
              <button type="button" className="normal secondary" onClick={close}>
                cancel
              </button>
              <button type="button" className="normal" onClick={confirmRemove}>
                remove
              </button>
            </div>
          </>
        )}
      />

      <ModalInner
        open={helpOpen}
        close={() => setHelpOpen(false)}
        title="editor help"
        contents={(close) => (
          <>
            <div className="help-content">
              <section>
                <h3>Rows</h3>
                <dl className="keybind-list">
                  <Keybind keys={["Ctrl", "↑"]} description="Move selected row up" />
                  <Keybind keys={["Ctrl", "↓"]} description="Move selected row down" />
                  <Keybind keys={["Ctrl", "Enter"]} description="Add row after selected" />
                </dl>
              </section>
              <section>
                <h3>Undo and redo</h3>
                <dl className="keybind-list">
                  <Keybind keys={["Ctrl", "z"]} description="Undo" />
                  <Keybind keys={["Ctrl", "Shift", "z"]} description="Redo" />
                  <Keybind keys={["Ctrl", "y"]} description="Redo" />
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
  );
}

export function mountDefinitionsEditor(
  containerId: string,
  options: {
    initialItems: Array<{ id: string; definition: string; context: string }>;
    isEdit: boolean;
  },
) {
  const container = document.getElementById(containerId);
  if (!container) {
    console.error(`Container #${containerId} not found`);
    return;
  }

  const itemsWithKeys: DefinitionItem[] = options.initialItems.map((item) => ({
    ...item,
    key: generateKey(),
  }));

  const root = ReactDOM.createRoot(container);
  root.render(<DefinitionsEditor initialItems={itemsWithKeys} isEdit={options.isEdit} />);
}

if (typeof window !== "undefined") {
  (window as any).mountDefinitionsEditor = mountDefinitionsEditor;
}
