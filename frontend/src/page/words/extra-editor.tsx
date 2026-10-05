import React, {
  createContext,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createRoot } from "react-dom/client";
import { Menu, MenuButton, MenuItem, MenuItems } from "@headlessui/react";
import { ModalInner } from "../../components/modal/modal";
import { Tooltip } from "../../components/tooltip/tooltip.tsx";
import { HelpIcon, Keybind } from "./editor-help";
import {
  parseJson,
  parseStructuredJson,
  parseStructuredNumber,
} from "./extra-editor-json";
import {
  applyJsonUpdate,
  fromTree,
  intoKey,
  intoTree,
  type JsonCollectionNode,
  type JsonKeyNode,
  type JsonNodeID,
  type JsonPrimitiveValues,
  type JsonTree,
  type JsonType,
  type JsonUpdate,
  type JsonUpdateResult,
  type JsonValue,
} from "./extra-editor-tree";

// Material Symbols by Google, matching the site's existing icons.
// https://github.com/google/material-design-icons/blob/master/LICENSE
const valueTypes: {
  type: JsonType;
  label: string;
  initial: JsonValue;
  iconPath: string;
}[] = [
  {
    type: "string",
    label: "Text",
    initial: "",
    iconPath:
      "M7.438 19.563Q7 19.125 7 18.5V7H3.5q-.625 0-1.062-.437T2 5.5t.438-1.062T3.5 4h10q.625 0 1.063.438T15 5.5t-.437 1.063T13.5 7H10v11.5q0 .625-.437 1.063T8.5 20t-1.062-.437m9 0Q16 19.125 16 18.5V12h-1.5q-.625 0-1.062-.437T13 10.5t.438-1.062T14.5 9h6q.625 0 1.063.438T22 10.5t-.437 1.063T20.5 12H19v6.5q0 .625-.437 1.063T17.5 20t-1.062-.437",
  },
  {
    type: "number",
    label: "Number",
    initial: 0,
    iconPath:
      "M5.5 10.5h-.75q-.325 0-.537-.213T4 9.75t.213-.537T4.75 9h1.5q.325 0 .538.213T7 9.75v4.5q0 .325-.213.538T6.25 15t-.537-.213t-.213-.537zM9 14.25V12.5q0-.425.288-.712T10 11.5h2v-1H9.75q-.325 0-.537-.213T9 9.75t.213-.537T9.75 9h2.75q.425 0 .713.288T13.5 10v1.5q0 .425-.288.713t-.712.287h-2v1h2.25q.325 0 .538.213t.212.537t-.213.538t-.537.212h-3q-.325 0-.537-.213T9 14.25m9.5.75h-2.75q-.325 0-.537-.213T15 14.25t.213-.537t.537-.213H18v-1h-1.5q-.2 0-.35-.15T16 12t.15-.35t.35-.15H18v-1h-2.25q-.325 0-.537-.213T15 9.75t.213-.537T15.75 9h2.75q.425 0 .713.288T19.5 10v4q0 .425-.288.713T18.5 15",
  },
  {
    type: "boolean",
    label: "Boolean",
    initial: false,
    iconPath:
      "m10.6 13.4l-2.15-2.15q-.275-.275-.7-.275t-.7.275t-.275.7t.275.7L9.9 15.5q.3.3.7.3t.7-.3l5.65-5.65q.275-.275.275-.7t-.275-.7t-.7-.275t-.7.275zM5 21q-.825 0-1.412-.587T3 19V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v14q0 .825-.587 1.413T19 21zm0-2h14V5H5zM5 5v14z",
  },
  {
    type: "object",
    label: "Object",
    initial: {},
    iconPath:
      "M15 20q-.425 0-.712-.288T14 19t.288-.712T15 18h2q.425 0 .713-.288T18 17v-2q0-.95.55-1.725t1.45-1.1v-.35q-.9-.325-1.45-1.1T18 9V7q0-.425-.288-.712T17 6h-2q-.425 0-.712-.288T14 5t.288-.712T15 4h2q1.25 0 2.125.875T20 7v2q0 .425.288.713T21 10t.713.288T22 11v2q0 .425-.288.713T21 14t-.712.288T20 15v2q0 1.25-.875 2.125T17 20zm-8 0q-1.25 0-2.125-.875T4 17v-2q0-.425-.288-.712T3 14t-.712-.288T2 13v-2q0-.425.288-.712T3 10t.713-.288T4 9V7q0-1.25.875-2.125T7 4h2q.425 0 .713.288T10 5t-.288.713T9 6H7q-.425 0-.712.288T6 7v2q0 .95-.55 1.725T4 11.825v.35q.9.325 1.45 1.1T6 15v2q0 .425.288.713T7 18h2q.425 0 .713.288T10 19t-.288.713T9 20z",
  },
  {
    type: "array",
    label: "List",
    initial: [],
    iconPath:
      "M10 19q-.425 0-.712-.288T9 18t.288-.712T10 17h10q.425 0 .713.288T21 18t-.288.713T20 19zm0-6q-.425 0-.712-.288T9 12t.288-.712T10 11h10q.425 0 .713.288T21 12t-.288.713T20 13zm0-6q-.425 0-.712-.288T9 6t.288-.712T10 5h10q.425 0 .713.288T21 6t-.288.713T20 7zM5 20q-.825 0-1.412-.587T3 18t.588-1.412T5 16t1.413.588T7 18t-.587 1.413T5 20m0-6q-.825 0-1.412-.587T3 12t.588-1.412T5 10t1.413.588T7 12t-.587 1.413T5 14M3.588 7.413Q3 6.825 3 6t.588-1.412T5 4t1.413.588T7 6t-.587 1.413T5 8t-1.412-.587",
  },
  {
    type: "null",
    label: "Null",
    initial: null,
    iconPath:
      "M12 22q-2.075 0-3.9-.788t-3.175-2.137T2.788 15.9T2 12t.788-3.9t2.137-3.175T8.1 2.788T12 2t3.9.788t3.175 2.137T21.213 8.1T22 12t-.788 3.9t-2.137 3.175t-3.175 2.138T12 22m0-2q3.35 0 5.675-2.325T20 12t-2.325-5.675T12 4T6.325 6.325T4 12t2.325 5.675T12 20m0-8",
  },
];

interface Draft {
  input: HTMLInputElement;
  dirty: () => boolean;
  validate: () => boolean;
  flush: () => boolean;
}
interface EditorServices {
  folded: ReadonlySet<JsonNodeID>;
  toggleFold: (node: JsonNodeID) => void;
  tree: () => JsonTree;
  update: (update: JsonUpdate) => JsonUpdateResult;
  register: (id: string, draft: Draft) => () => void;
  controlId: (node: JsonNodeID, control: string) => string;
  focus: (id: string) => void;
  draftChanged: () => void;
}
const EditorContext = createContext<EditorServices | null>(null);
function useEditor() {
  const services = useContext(EditorContext);
  if (!services) throw new Error("JSON controls require an editor owner");
  return services;
}

// Draft text can be invalid; only a successful commit changes the JSON tree.
function DraftInput({
  id,
  className,
  value,
  label,
  validate,
  commit,
  normalize = (text) => text,
}: {
  id: string;
  className?: string;
  value: string;
  label: string;
  validate: (text: string) => string | null;
  commit: (text: string) => JsonUpdateResult;
  normalize?: (text: string) => string;
}) {
  const services = useEditor();
  const [draft, setDraft] = useState(value);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const errorId = `${id}-error`;
  const showError = (message: string | null) => {
    input.current?.setCustomValidity(message ?? "");
    setError(message);
    return message === null;
  };
  const check = () => showError(validate(input.current?.value ?? draft));
  const flush = () => {
    const text = input.current?.value ?? draft;
    if (!showError(validate(text))) return false;
    const result = commit(text);
    if (result.ok) {
      const normalized = normalize(text);
      if (normalized !== text) {
        if (input.current) input.current.value = normalized;
        setDraft(normalized);
        services.draftChanged();
      }
    }
    return showError(result.ok ? null : result.error);
  };

  useLayoutEffect(() => {
    setDraft(value);
    if (input.current) input.current.value = value;
    services.draftChanged();
  }, [value]);
  // Revalidate keys against current siblings, including after conflict removal.
  useLayoutEffect(() => {
    check();
    if (!input.current) return;
    return services.register(id, {
      input: input.current,
      dirty: () => input.current?.value !== value,
      validate: check,
      flush,
    });
  });

  return (
    <div className="extra-editor-field">
      <input
        ref={input}
        id={id}
        className={`${className ?? ""}${error ? " error" : ""}`}
        type="text"
        value={draft}
        aria-label={label}
        onChange={(event) => {
          const text = event.currentTarget.value;
          setDraft(text);
          showError(validate(text));
          services.draftChanged();
        }}
        onBlur={() => {
          flush();
        }}
        aria-invalid={error !== null}
        aria-describedby={error ? errorId : undefined}
      />
      {error && (
        <span id={errorId} className="extra-editor-error">
          {error}
        </span>
      )}
    </div>
  );
}

type PrimitiveEditorProps = {
  [T in keyof JsonPrimitiveValues]: {
    type: T;
    value: JsonPrimitiveValues[T];
    onChange: (value: JsonPrimitiveValues[T]) => JsonUpdateResult;
    id: string;
    label: string;
  };
}[keyof JsonPrimitiveValues];

function PrimitiveEditor(props: PrimitiveEditorProps) {
  switch (props.type) {
    case "boolean":
      return (
        <div className="extra-editor-boolean">
          <input
            id={props.id}
            className="extra-editor-boolean-input"
            type="checkbox"
            checked={props.value}
            onChange={(e) => {
              props.onChange(e.currentTarget.checked);
            }}
            aria-label={props.label}
          />
        </div>
      );
    case "null":
      return (
        <p className="extra-editor-null">
          <code aria-label={props.label}>null</code>
        </p>
      );
    case "string":
      return (
        <DraftInput
          className="extra-editor-string"
          id={props.id}
          label={`${props.label} (Text)`}
          value={props.value}
          validate={() => null}
          commit={props.onChange}
        />
      );
    case "number":
      return (
        <DraftInput
          className="extra-editor-number"
          id={props.id}
          label={`${props.label} (Number)`}
          value={String(props.value)}
          validate={(text) => {
            try {
              parseStructuredNumber(text);
              return null;
            } catch (error) {
              return String((error as Error).message);
            }
          }}
          commit={(text) => props.onChange(parseStructuredNumber(text))}
          normalize={(text) => String(parseStructuredNumber(text))}
        />
      );
  }
}

function AddMenu({
  id,
  label,
  onAdd,
  iconOnly = false,
  className = iconOnly ? "icon gray" : "normal extra-editor-add",
}: {
  id: string;
  label: string;
  onAdd: (value: JsonValue) => void;
  iconOnly?: boolean;
  className?: string;
}) {
  return (
    <Menu>
      <MenuButton
        id={id}
        type="button"
        className={className}
        aria-label={label}
        title={iconOnly ? label : undefined}
      >
        <svg className="icon" aria-hidden="true">
          <use href="#icon-plus" />
        </svg>
        {!iconOnly && "add"}
      </MenuButton>
      <MenuItems
        transition
        anchor={{ to: "bottom start", gap: 4, padding: 8 }}
        modal={false}
        className="extra-editor-type-menu"
      >
        {valueTypes.map(({ type, label: typeLabel, initial, iconPath }) => (
          <MenuItem key={type}>
            {({ close }) => (
              <button
                type="button"
                onClick={() => {
                  close();
                  onAdd(initial);
                }}
              >
                <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">
                  <path fill="currentColor" d={iconPath} />
                </svg>
                <span>{typeLabel}</span>
              </button>
            )}
          </MenuItem>
        ))}
      </MenuItems>
    </Menu>
  );
}

function CollectionAddMenu({
  node,
  label,
  position = "end",
  className,
}: {
  node: JsonNodeID;
  label: string;
  position?: "start" | "end";
  className?: string;
}) {
  const services = useEditor();
  const add = (value: JsonValue) => {
    const parent = services.tree().nodes[node];
    if (parent?.kind !== "collection") return;
    const index = position === "start" ? 0 : parent.children.length;
    const names = new Set(
      parent.children.map((id) => {
        const entry = services.tree().nodes[id];
        return entry?.kind === "key" ? entry.key : null;
      }),
    );
    let name = "key";
    for (let suffix = 2; names.has(intoKey(name)); suffix++)
      name = `key${suffix}`;
    const result = services.update({
      kind: "insert",
      value,
      destination:
        parent.type === "array"
          ? { type: "array", parent: parent.id, index }
          : { type: "object", parent: parent.id, index, key: intoKey(name) },
    });
    if (!result.ok) return;
    const next = result.tree.nodes[node];
    if (next?.kind !== "collection") return;
    const entry = result.tree.nodes[next.children[index]!];
    if (!entry) return;
    if (services.folded.has(node)) services.toggleFold(node);
    const control =
      entry.kind === "key"
        ? "key"
        : entry.kind === "collection"
          ? "add-end"
          : entry.kind === "primitive" && entry.type === "null"
            ? "remove"
            : "value";
    services.focus(
      services.controlId(
        entry.kind === "key" ? entry.value : entry.id,
        control,
      ),
    );
  };
  return (
    <AddMenu
      id={services.controlId(
        node,
        position === "start" ? "add-start" : "add-end",
      )}
      label={`${label}: add at ${position === "start" ? "beginning" : "end"}`}
      iconOnly={position === "start"}
      className={className}
      onAdd={add}
    />
  );
}

function propertyLabel(key: string) {
  return key === "" ? "empty property name" : JSON.stringify(key);
}

function FoldButton({
  node,
  label,
  className = "icon gray",
}: {
  node: JsonNodeID;
  label: string;
  className?: string;
}) {
  const services = useEditor();
  const folded = services.folded.has(node);
  const action = folded ? "expand" : "collapse";
  return (
    <Tooltip content={action}>
      <button
        type="button"
        id={services.controlId(node, "fold")}
        className={className}
        aria-label={`${label}: ${action}`}
        aria-expanded={!folded}
        aria-controls={services.controlId(node, "collection")}
        onClick={() => services.toggleFold(node)}
      >
        <svg className="icon" aria-hidden="true">
          <use href={folded ? "#icon-chevron-right" : "#icon-chevron-down"} />
        </svg>
      </button>
    </Tooltip>
  );
}

function KeyEditor({
  node,
  parent,
  label,
}: {
  node: JsonKeyNode;
  parent: JsonNodeID;
  label: string;
}) {
  const services = useEditor();
  return (
    <DraftInput
      className="extra-editor-key"
      id={services.controlId(node.value, "key")}
      value={node.key}
      label={`${label}: property name`}
      validate={(text) => {
        const collection = services.tree().nodes[parent];
        if (collection?.kind !== "collection")
          return "Property no longer exists.";
        return collection.children.some((id) => {
          const sibling = services.tree().nodes[id];
          return (
            sibling?.kind === "key" &&
            sibling.id !== node.id &&
            sibling.key === text
          );
        })
          ? "This object already has that property name."
          : null;
      }}
      commit={(text) =>
        services.update({
          kind: "rename-key",
          node: node.id,
          key: intoKey(text),
        })
      }
    />
  );
}

function ValueEditor({
  tree,
  nodeId,
  label,
}: {
  tree: JsonTree;
  nodeId: JsonNodeID;
  label: string;
}) {
  const services = useEditor();
  const node = tree.nodes[nodeId];
  if (!node || node.kind === "key") throw new Error("Expected JSON value");
  const folded = services.folded.has(nodeId);
  if (node.kind === "collection")
    return (
      <>
        <button
          type="button"
          id={services.controlId(nodeId, "summary")}
          className="extra-editor-collection-summary"
          aria-label={`${label}: ${folded ? "expand" : "collapse"} collection`}
          aria-expanded={!folded}
          aria-controls={services.controlId(nodeId, "collection")}
          onClick={() => services.toggleFold(nodeId)}
        >
          <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d={valueTypes.find(({ type }) => type === node.type)?.iconPath}
            />
          </svg>
          <span>
            {node.type === "array" ? "List" : "Object"} ({node.children.length}{" "}
            {node.type === "array"
              ? node.children.length === 1
                ? "item"
                : "items"
              : node.children.length === 1
                ? "property"
                : "properties"}
            )
          </span>
        </button>
        <CollectionEditor tree={tree} node={node} label={label} />
      </>
    );
  const onChange = (value: JsonValue) =>
    services.update({ kind: "replace", node: nodeId, value });
  return (
    <PrimitiveEditor
      {...node}
      id={services.controlId(nodeId, "value")}
      label={label}
      onChange={onChange}
    />
  );
}

export interface CollectionEditorProps {
  tree: JsonTree;
  node: JsonCollectionNode;
  label: string;
}

export function CollectionEditor({ tree, node, label }: CollectionEditorProps) {
  const services = useEditor();
  const groupId = services.controlId(node.id, "label");
  const current = () => {
    const collection = services.tree().nodes[node.id];
    if (collection?.kind !== "collection")
      throw new Error("Collection no longer exists");
    return collection;
  };
  const primaryControl = (id: JsonNodeID) => {
    const value = services.tree().nodes[id];
    if (value?.kind === "collection")
      return services.controlId(
        id,
        services.folded.has(id) ? "fold" : "add-end",
      );
    return services.controlId(
      id,
      value?.kind === "primitive" && value.type === "null" ? "remove" : "value",
    );
  };
  const remove = (id: JsonNodeID, entryId: JsonNodeID) => {
    const parent = current();
    const index = parent.children.indexOf(entryId);
    const neighbor = parent.children[index + 1] ?? parent.children[index - 1];
    const result = services.update({ kind: "remove", node: id });
    if (!result.ok) return;
    const next = neighbor ? result.tree.nodes[neighbor] : undefined;
    services.focus(
      next
        ? next.kind === "key"
          ? services.controlId(next.value, "key")
          : primaryControl(next.id)
        : services.controlId(node.id, "add-end"),
    );
  };
  const move = (id: JsonNodeID, direction: -1 | 1, focusId?: string) => {
    const parent = current();
    const index = parent.children.indexOf(id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= parent.children.length) return;
    const result = services.update({
      kind: "move",
      node: id,
      destination: { type: "array", parent: parent.id, index: target },
    });
    if (!result.ok) return;
    services.focus(
      focusId ?? services.controlId(id, direction < 0 ? "up" : "down"),
    );
  };
  const List = node.type === "array" ? "ol" : "ul";
  return (
    <div
      id={services.controlId(node.id, "collection")}
      className={`extra-editor-collection extra-editor-${node.type}`}
      hidden={services.folded.has(node.id)}
      role="group"
      aria-label={label}
    >
      <List className="extra-editor-entries">
        {node.children.map((entryId, index) => {
          const entry = tree.nodes[entryId];
          if (!entry) throw new Error("Missing JSON entry");
          const valueId = entry.kind === "key" ? entry.value : entry.id;
          const isCollectionEntry = tree.nodes[valueId]?.kind === "collection";
          const entryLabel =
            entry.kind === "key"
              ? `${label}, property ${propertyLabel(entry.key)}`
              : `${label}, item ${index + 1}`;
          return (
            <li
              key={valueId}
              className={`extra-editor-entry${isCollectionEntry ? " extra-editor-entry-collection" : ""}`}
              onKeyDown={(event) => {
                if (
                  node.type !== "array" ||
                  event.defaultPrevented ||
                  event.altKey ||
                  (!event.ctrlKey && !event.metaKey) ||
                  (event.key !== "ArrowUp" && event.key !== "ArrowDown") ||
                  !(event.target instanceof HTMLElement) ||
                  event.target.closest(".extra-editor-entry") !==
                    event.currentTarget
                )
                  return;
                event.preventDefault();
                event.stopPropagation();
                move(
                  valueId,
                  event.key === "ArrowUp" ? -1 : 1,
                  event.target.id || undefined,
                );
              }}
            >
              {entry.kind === "key" && (
                <KeyEditor node={entry} parent={node.id} label={entryLabel} />
              )}
              <ValueEditor tree={tree} nodeId={valueId} label={entryLabel} />
              <div className="extra-editor-actions">
                {node.type === "array" && (
                  <>
                    <Tooltip content="move up">
                      <button
                        type="button"
                        id={services.controlId(valueId, "up")}
                        className="icon gray"
                        aria-label={`${entryLabel}: move up`}
                        aria-disabled={index === 0}
                        onClick={() => move(valueId, -1)}
                      >
                        <svg className="icon" aria-hidden="true">
                          <use href="#icon-arrow-upward" />
                        </svg>
                      </button>
                    </Tooltip>
                    <Tooltip content="move down">
                      <button
                        type="button"
                        id={services.controlId(valueId, "down")}
                        className="icon gray"
                        aria-label={`${entryLabel}: move down`}
                        aria-disabled={index === node.children.length - 1}
                        onClick={() => move(valueId, 1)}
                      >
                        <svg className="icon" aria-hidden="true">
                          <use href="#icon-arrow-downward" />
                        </svg>
                      </button>
                    </Tooltip>
                  </>
                )}
                {isCollectionEntry && (
                  <>
                    <CollectionAddMenu
                      node={valueId}
                      label={entryLabel}
                      position="start"
                      className="icon gray extra-editor-add-start"
                    />
                    <FoldButton node={valueId} label={entryLabel} />
                  </>
                )}
                <Tooltip content="remove">
                  <button
                    type="button"
                    id={services.controlId(valueId, "remove")}
                    className="icon gray"
                    aria-label={`${entryLabel}: remove`}
                    onClick={() => remove(valueId, entryId)}
                  >
                    <svg aria-hidden="true">
                      <use href="#icon-close-small" />
                    </svg>
                  </button>
                </Tooltip>
              </div>
            </li>
          );
        })}
      </List>
      <div className="extra-editor-collection-footer">
        <CollectionAddMenu node={node.id} label={label} />
      </div>
    </div>
  );
}

interface EditorSnapshot {
  tree: JsonTree | null;
  raw: string;
  mode: "empty" | "structured" | "raw";
}
interface EditorState extends EditorSnapshot {
  undo: EditorSnapshot[];
  redo: EditorSnapshot[];
  revision: number;
}
function parseTree(text: string) {
  return intoTree(parseStructuredJson(text));
}
function initialState(raw: string): EditorState {
  let tree: JsonTree | null = null;
  try {
    tree = parseTree(raw);
  } catch {
    /* Preserve text that is invalid or cannot round-trip through form fields. */
  }
  return {
    tree,
    raw,
    mode: raw.trim() === "" ? "empty" : tree ? "structured" : "raw",
    undo: [],
    redo: [],
    revision: 0,
  };
}

function snapshot({ tree, raw, mode }: EditorState): EditorSnapshot {
  return { tree, raw, mode };
}
function serializedValue(state: EditorSnapshot) {
  if (state.mode === "empty") return "";
  return state.mode === "raw"
    ? state.raw
    : JSON.stringify(fromTree(state.tree!));
}

export function ExtraEditor({
  initialValue,
  fieldId = "extra",
  label = "Extra JSON (optional)",
}: {
  initialValue: string;
  fieldId?: string;
  label?: string;
}) {
  const [state, setState] = useState(() => initialState(initialValue));
  const [helpOpen, setHelpOpen] = useState(false);
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [folded, setFolded] = useState<ReadonlySet<JsonNodeID>>(
    () => new Set(),
  );
  const latest = useRef(state);
  const root = useRef<HTMLDivElement>(null);
  const successfulControl = useRef<HTMLInputElement | HTMLTextAreaElement>(
    null,
  );
  const drafts = useRef(new Map<string, Draft>());
  const [, setDraftRevision] = useState(0);
  const focusToken = useRef(0);
  const pendingFocus = useRef<string | null>(null);
  const prefix = useId();
  const publish = (next: EditorState) => {
    latest.current = next;
    if (successfulControl.current)
      successfulControl.current.value = serializedValue(next);
    setState(next);
    // Native validation runs before submit, so reflect sibling-key changes now.
    for (const draft of drafts.current.values()) {
      if (next.mode === "structured" && draft.input.isConnected)
        draft.validate();
    }
  };
  const services = useMemo<EditorServices>(
    () => ({
      folded,
      toggleFold: (node) => {
        if (!folded.has(node)) {
          const collection = document.getElementById(
            services.controlId(node, "collection"),
          );
          // Keep invalid drafts visible so native form validation can focus them.
          const invalid = [...drafts.current.values()].find(
            (draft) => collection?.contains(draft.input) && !draft.validate(),
          );
          if (invalid) {
            invalid.input.focus();
            return;
          }
        }
        setFolded((previous) => {
          const next = new Set(previous);
          if (next.has(node)) next.delete(node);
          else next.add(node);
          return next;
        });
      },
      tree: () => {
        if (!latest.current.tree) throw new Error("No structured JSON");
        return latest.current.tree;
      },
      update: (update) => {
        const previous = latest.current;
        if (!previous.tree) return { ok: false, error: "No structured JSON" };
        const node =
          "node" in update ? previous.tree.nodes[update.node] : undefined;
        if (
          (update.kind === "replace" &&
            node?.kind === "primitive" &&
            node.value === update.value) ||
          (update.kind === "rename-key" &&
            node?.kind === "key" &&
            node.key === update.key)
        )
          return { ok: true, tree: previous.tree };
        const result = applyJsonUpdate(previous.tree, update);
        if (result.ok)
          publish({
            ...previous,
            tree: result.tree,
            undo: [...previous.undo.slice(-99), snapshot(previous)],
            redo: [],
          });
        return result;
      },
      register: (id, draft) => {
        drafts.current.set(id, draft);
        return () => {
          if (drafts.current.get(id) === draft) drafts.current.delete(id);
        };
      },
      controlId: (node, control) => `${prefix}-${node}-${control}`,
      focus: (id) => {
        const token = ++focusToken.current;
        // Headless UI also restores focus after closing a menu. Wait for its
        // restoration, and cancel if another keyboard/pointer action intervenes.
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            if (token !== focusToken.current) return;
            const element = document.getElementById(id);
            if (element && root.current?.contains(element)) element.focus();
          }),
        );
      },
      draftChanged: () => setDraftRevision((previous) => previous + 1),
    }),
    [folded],
  );

  const flushDrafts = () => {
    const ordered = [...drafts.current.values()]
      .filter((draft) => draft.input.isConnected)
      .sort((a, b) =>
        a.input.compareDocumentPosition(b.input) &
        Node.DOCUMENT_POSITION_FOLLOWING
          ? -1
          : 1,
      );
    const invalid = ordered.find((draft) => !draft.validate());
    if (invalid) {
      invalid.input.focus();
      return false;
    }
    for (const draft of ordered) {
      if (!draft.flush()) {
        draft.input.focus();
        return false;
      }
    }
    return true;
  };
  const rawError = (text: string) => {
    if (text.trim() === "") return null;
    try {
      parseJson(text);
      return null;
    } catch {
      return "Enter valid JSON with finite numbers, or leave blank to clear extra data.";
    }
  };
  const error = state.mode === "raw" ? rawError(state.raw) : null;
  // Mode changes replace the focused control. Restore focus during the commit
  // so another shortcut works immediately, without the menu restoration delay.
  const focusAfterRender = (id: string) => {
    focusToken.current++;
    pendingFocus.current = id;
  };
  useLayoutEffect(() => {
    const id = pendingFocus.current;
    pendingFocus.current = null;
    if (!id) return;
    const element = document.getElementById(id);
    if (element && root.current?.contains(element)) element.focus();
  });
  useLayoutEffect(() => {
    successfulControl.current?.setCustomValidity(error ?? "");
  }, [error, state.mode]);
  useEffect(() => {
    const form = root.current?.closest("form");
    const submit = (event: SubmitEvent) => {
      if (latest.current.mode === "structured" && !flushDrafts())
        event.preventDefault();
    };
    const cancelFocus = () => {
      focusToken.current++;
    };
    form?.addEventListener("submit", submit, true);
    document.addEventListener("keydown", cancelFocus, true);
    document.addEventListener("pointerdown", cancelFocus, true);
    return () => {
      form?.removeEventListener("submit", submit, true);
      document.removeEventListener("keydown", cancelFocus, true);
      document.removeEventListener("pointerdown", cancelFocus, true);
      focusToken.current++;
    };
  }, []);

  const changeRoot = (next: EditorSnapshot) => {
    const previous = latest.current;
    publish({
      ...previous,
      ...next,
      undo: [...previous.undo.slice(-99), snapshot(previous)],
      redo: [],
      revision: previous.revision + 1,
    });
  };
  const openEditor = () => {
    const tree = intoTree({});
    changeRoot({ mode: "structured", tree, raw: "" });
    focusAfterRender(services.controlId(tree.root, "add-end"));
  };
  const clearRoot = () => {
    // Like removing a nested value, clearing discards unfinished drafts.
    changeRoot({ mode: "empty", tree: null, raw: "" });
    focusAfterRender(`${prefix}-open`);
  };
  const requestClearRoot = () => {
    const current = latest.current;
    let emptyObject = false;
    if (current.mode === "structured" && current.tree) {
      const node = current.tree.nodes[current.tree.root];
      emptyObject =
        node?.kind === "collection" &&
        node.type === "object" &&
        node.children.length === 0;
    } else if (current.mode === "raw") {
      try {
        const value = parseJson(current.raw);
        emptyObject =
          value !== null &&
          typeof value === "object" &&
          !Array.isArray(value) &&
          Object.keys(value).length === 0;
      } catch {
        // Invalid raw JSON still has content to confirm discarding.
      }
    }
    if (emptyObject) clearRoot();
    else setConfirmClearOpen(true);
  };
  const history = (direction: "undo" | "redo") => {
    // Unavailable Redo must not commit a draft just because it was pressed.
    if (direction === "redo" && latest.current.redo.length === 0) return;
    if (latest.current.mode === "structured" && !flushDrafts()) return;
    const previous = latest.current;
    const source = previous[direction];
    const restored = source.at(-1);
    if (!restored) return;
    const opposite = direction === "undo" ? "redo" : "undo";
    publish({
      ...previous,
      ...restored,
      [direction]: source.slice(0, -1),
      [opposite]: [...previous[opposite].slice(-99), snapshot(previous)],
      revision: previous.revision + 1,
    });
    if (restored.mode !== previous.mode) {
      focusAfterRender(
        restored.mode === "empty"
          ? `${prefix}-open`
          : restored.mode === "raw"
            ? fieldId
            : `${prefix}-remove-root`,
      );
    }
  };
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.defaultPrevented || helpOpen || confirmClearOpen || event.altKey)
      return;
    if (!event.ctrlKey && !event.metaKey) return;
    const key = event.key.toLowerCase();
    if (key !== "z" && key !== "y") return;
    // Match Definitions: leave native text history alone inside fields.
    if (
      event.target instanceof HTMLElement &&
      (event.target.matches("input, textarea") ||
        event.target.isContentEditable)
    )
      return;
    event.preventDefault();
    history(key === "y" || event.shiftKey ? "redo" : "undo");
  };
  const hasDirtyDrafts = [...drafts.current.values()].some(
    (draft) => draft.input.isConnected && draft.dirty(),
  );
  return (
    <EditorContext.Provider value={services}>
      <div
        ref={root}
        className={`extra-editor${state.mode === "empty" ? " extra-editor-empty" : ""}`}
        onKeyDown={onKeyDown}
      >
        {state.mode !== "empty" && (
          <div className="extra-editor-heading">
            {state.mode === "raw" ? (
              <label htmlFor={fieldId}>{label}</label>
            ) : (
              <span id={`${prefix}-label`}>{label}</span>
            )}
            <div className="def-controls">
              <span className="controls-header">Editor</span>
              <Tooltip content="undo">
                <button
                  id={`${prefix}-undo`}
                  type="button"
                  className="control-button first"
                  disabled={state.undo.length === 0 && !hasDirtyDrafts}
                  aria-label="Undo"
                  onClick={() => history("undo")}
                >
                  <svg className="icon" aria-hidden="true">
                    <use href="#icon-undo" />
                  </svg>
                </button>
              </Tooltip>
              <Tooltip content="redo">
                <button
                  type="button"
                  className="control-button"
                  disabled={state.redo.length === 0}
                  aria-label="Redo"
                  onClick={() => history("redo")}
                >
                  <svg className="icon" aria-hidden="true">
                    <use href="#icon-redo" />
                  </svg>
                </button>
              </Tooltip>
              {state.mode === "structured" &&
                state.tree?.nodes[state.tree.root]?.kind === "collection" && (
                  <>
                    <CollectionAddMenu
                      node={state.tree.root}
                      label="Extra"
                      position="start"
                      className="control-button extra-editor-add-start"
                    />
                    <FoldButton
                      node={state.tree.root}
                      label="Extra"
                      className="control-button"
                    />
                  </>
                )}
              <Tooltip content="remove extra data">
                <button
                  id={`${prefix}-remove-root`}
                  type="button"
                  className="control-button"
                  aria-label="Remove extra data"
                  onClick={requestClearRoot}
                >
                  <svg className="icon" aria-hidden="true">
                    <use href="#icon-delete" />
                  </svg>
                </button>
              </Tooltip>
              <Tooltip content="editor help">
                <button
                  type="button"
                  className="control-button"
                  aria-label="Extra editor help"
                  onClick={() => setHelpOpen(true)}
                >
                  <HelpIcon />
                </button>
              </Tooltip>
            </div>
          </div>
        )}
        {state.mode === "empty" && (
          <button
            id={`${prefix}-open`}
            type="button"
            className="extra-editor-open"
            onClick={openEditor}
          >
            <svg className="icon" aria-hidden="true">
              <use href="#icon-plus" />
            </svg>
            Add extra data
          </button>
        )}
        {state.mode === "raw" ? (
          <>
            <textarea
              ref={successfulControl as React.Ref<HTMLTextAreaElement>}
              id={fieldId}
              className="extra-editor-raw"
              name="extra"
              rows={8}
              spellCheck={false}
              value={state.raw}
              aria-invalid={error !== null}
              aria-describedby={
                error ? `${prefix}-raw-error ${prefix}-hint` : `${prefix}-hint`
              }
              onChange={(event) => {
                event.currentTarget.setCustomValidity(
                  rawError(event.currentTarget.value) ?? "",
                );
                const raw = event.currentTarget.value;
                if (raw.trim() === "") {
                  clearRoot();
                } else {
                  changeRoot({ mode: "raw", tree: null, raw });
                }
              }}
            />
            {error && (
              <span className="extra-editor-error" id={`${prefix}-raw-error`}>
                {error}
              </span>
            )}
          </>
        ) : state.mode === "structured" ? (
          <>
            <div
              key={state.revision}
              role="group"
              aria-labelledby={`${prefix}-label`}
              aria-describedby={`${prefix}-hint`}
            >
              <ValueEditor
                tree={state.tree!}
                nodeId={state.tree!.root}
                label="Extra"
              />
            </div>
            <input
              ref={successfulControl as React.Ref<HTMLInputElement>}
              type="hidden"
              name="extra"
              value={serializedValue(state)}
            />
          </>
        ) : (
          <input
            ref={successfulControl as React.Ref<HTMLInputElement>}
            type="hidden"
            name="extra"
            value=""
          />
        )}
        <ModalInner
          open={confirmClearOpen}
          close={() => setConfirmClearOpen(false)}
          title="Clear all extra data?"
          contents={(close) => (
            <>
              <p>
                Are you sure you want to clear all extra data? This removes the
                whole value, including all nested properties and list items.
              </p>
              <div className="button-row">
                <button
                  type="button"
                  className="normal secondary"
                  data-autofocus
                  onClick={close}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="normal"
                  onClick={() => {
                    close();
                    clearRoot();
                  }}
                >
                  Clear all data
                </button>
              </div>
            </>
          )}
        />
        <ModalInner
          open={helpOpen}
          close={() => setHelpOpen(false)}
          title="Extra editor help"
          contents={(close) => (
            <>
              <div className="help-content">
                <section>
                  <h3>List rows</h3>
                  <dl className="keybind-list">
                    <Keybind
                      keys={["Ctrl", "↑"]}
                      description="Move focused row up"
                    />
                    <Keybind
                      keys={["Ctrl", "↓"]}
                      description="Move focused row down"
                    />
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
                <button type="button" className="normal" onClick={close}>
                  Close
                </button>
              </div>
            </>
          )}
        />
      </div>
    </EditorContext.Provider>
  );
}

export function mountExtraEditor(containerId: string) {
  const container = document.getElementById(containerId);
  const textarea = container?.querySelector<HTMLTextAreaElement>(
    "textarea[name=extra]",
  );
  if (!container || !textarea) return;
  const label = textarea.labels?.[0]?.textContent?.trim();
  createRoot(container).render(
    <ExtraEditor
      initialValue={textarea.value}
      fieldId={textarea.id || "extra"}
      label={label}
    />,
  );
}

declare global {
  interface Window {
    mountExtraEditor?: typeof mountExtraEditor;
  }
}
if (typeof window !== "undefined") window.mountExtraEditor = mountExtraEditor;
