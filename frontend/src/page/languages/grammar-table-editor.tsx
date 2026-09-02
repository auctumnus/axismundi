import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import ReactDOM from "react-dom/client";
import { EditorState } from "@codemirror/state";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import {
  indentUnit,
  syntaxHighlighting,
  defaultHighlightStyle,
} from "@codemirror/language";
import { acceptCompletion, completionKeymap } from "@codemirror/autocomplete";
import { ModalInner } from "../../components/modal/modal";
import {
  WordCombobox,
  type WordOption,
} from "../../components/combobox/word-combobox";
import { ControlButton } from "./phonology-editor/controls";
import { Help } from "./phonology-editor/help";
import {
  declaredLexurgyNames,
  lexurgy,
} from "../sound-changes/runner/lexurgy-language";
import { theme } from "../sound-changes/runner/editor-theme";
import {
  apply,
  cellColspan,
  cellRowspan,
  coveredCells,
  flatRows,
  headingAt,
  headingDepth,
  initialState,
  leafCount,
  movementFromKey,
  moveFocus,
  serializeFocus,
  type Action,
  type Body,
  type Column,
  type Focus,
  type GridCell,
  type HeadingPath,
  type Row,
  type State,
} from "./table-editor-core";

type Cell = GridCell & { changes: string };
type GrammarBody = Body<Cell>;
type Preview =
  | { kind: "empty" }
  | { kind: "running" }
  | { kind: "timed_out" }
  | { kind: "value"; value: string; ipa?: string }
  | { kind: "error"; message: string };
type PreviewExample = Pick<WordOption, "word" | "ipa" | "extra">;
type PreviewRunner = (
  example: PreviewExample,
  preamble: string,
  changes: string,
) => Promise<Preview>;
const options = {
  createCell: (): Cell => ({ changes: "" }),
  mergeCells: (anchor: Cell): Cell => ({ ...anchor }),
};
const completeWithTab = { key: "Tab", run: acceptCompletion };
const noExternalDeclarations: string[] = [];

function SoundChangeEditor({
  value,
  onChange,
  label,
  externalDeclarations = noExternalDeclarations,
}: {
  value: string;
  onChange: (value: string) => void;
  label: string;
  externalDeclarations?: string[];
}) {
  const host = useRef<HTMLDivElement>(null);
  const callback = useRef(onChange);
  callback.current = onChange;
  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          indentUnit.of("  "),
          lineNumbers(),
          history(),
          drawSelection(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          keymap.of([
            ...defaultKeymap,
            ...historyKeymap,
            ...completionKeymap,
            completeWithTab,
            indentWithTab,
          ]),
          lexurgy({ externalDeclarations }),
          syntaxHighlighting(defaultHighlightStyle),
          theme,
          EditorView.updateListener.of((update) => {
            if (update.docChanged)
              callback.current(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.contentDOM.setAttribute("aria-label", label);
    view.contentDOM.setAttribute("role", "textbox");
    view.contentDOM.setAttribute("aria-multiline", "true");
    return () => view.destroy();
    // Deliberately switch documents when the selected cell changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label, externalDeclarations]);
  return (
    <div className="changes-editor-container grammar-code-editor" ref={host} />
  );
}

const wait = (milliseconds: number) =>
  new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));

function usePreviewRunner(previewUrl: string): PreviewRunner {
  const cache = useRef(new Map<string, Preview>());
  const inFlight = useRef(new Map<string, Promise<Preview>>());
  const queue = useRef(Promise.resolve());
  const lastRequestAt = useRef(0);

  return useCallback(
    (example, preamble, changes) => {
      const key = JSON.stringify([previewUrl, example, preamble, changes]);
      const cached = cache.current.get(key);
      if (cached) return Promise.resolve(cached);
      const existing = inFlight.current.get(key);
      if (existing) return existing;

      const request = async (): Promise<Preview> => {
        const gap = 550 - (Date.now() - lastRequestAt.current);
        if (gap > 0) await wait(gap);
        lastRequestAt.current = Date.now();

        for (let attempt = 0; attempt < 3; attempt += 1) {
          const response = await fetch(previewUrl, {
            method: "POST",
            credentials: "same-origin",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              input: example.word,
              ipa: example.ipa,
              extra: example.extra,
              preamble,
              changes,
            }),
          });
          if (response.status === 429 && attempt < 2) {
            const retryAfter = Number(response.headers.get("retry-after"));
            await wait(
              Number.isFinite(retryAfter) && retryAfter > 0
                ? retryAfter * 1000
                : 1_000 * (attempt + 1),
            );
            lastRequestAt.current = Date.now();
            continue;
          }
          if (!response.ok)
            throw new Error(
              response.status === 429
                ? "preview rate limit reached; please wait a moment"
                : "preview request failed",
            );
          const result = await response.json();
          if (result.status === "rendered")
            return {
              kind: "value",
              value: result.value,
              ipa: result.ipa || undefined,
            };
          if (result.status === "timed_out") return { kind: "timed_out" };
          return {
            kind: "error",
            message: result.message || "could not run these rules",
          };
        }
        return {
          kind: "error",
          message: "preview rate limit reached; please wait a moment",
        };
      };

      const task = queue.current.then(request);
      queue.current = task.then(
        () => undefined,
        () => undefined,
      );
      inFlight.current.set(key, task);
      void task
        .then((result) => cache.current.set(key, result))
        .finally(() => inFlight.current.delete(key));
      return task;
    },
    [previewUrl],
  );
}

function PreviewCell({
  cell,
  preamble,
  example,
  hasIpaEstimator,
  runPreview,
  onPreviewChange,
}: {
  cell: Cell;
  preamble: string;
  example: PreviewExample | null;
  hasIpaEstimator: boolean;
  runPreview: PreviewRunner;
  onPreviewChange: (preview: Preview) => void;
}) {
  const [preview, setPreview] = useState<Preview>({ kind: "empty" });
  useEffect(() => {
    if (!example?.word.trim()) {
      setPreview({ kind: "empty" });
      return;
    }
    // With an estimator, even a cell that changes nothing shows the IPA of
    // that completed form, so the server has to run.
    if (!hasIpaEstimator && !preamble.trim() && !cell.changes.trim()) {
      setPreview({
        kind: "value",
        value: example.word.trim(),
      });
      return;
    }
    let cancelled = false;
    setPreview({ kind: "running" });
    const timer = window.setTimeout(() => {
      void runPreview({ ...example, word: example.word.trim() }, preamble, cell.changes)
        .then((result) => {
          if (!cancelled) setPreview(result);
        })
        .catch((error) => {
          if (!cancelled)
            setPreview({
              kind: "error",
              message:
                error instanceof Error
                  ? error.message
                  : "could not run these rules",
            });
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [cell.changes, example, hasIpaEstimator, preamble, runPreview]);
  useEffect(() => onPreviewChange(preview), [onPreviewChange, preview]);
  if (preview.kind === "empty")
    return <span className="grammar-preview-muted">enter an example</span>;
  if (preview.kind === "running")
    return (
      <span className="grammar-preview-running">
        <span className="grammar-preview-spinner" />
        running…
      </span>
    );
  if (preview.kind === "timed_out")
    return <span className="grammar-preview-error">preview timed out</span>;
  if (preview.kind === "error")
    return (
      <span className="grammar-preview-error" title={preview.message}>
        preview error
      </span>
    );
  return (
    <>
      <span>{preview.value}</span>
      {preview.ipa && (
        <span className="grammar-cell-ipa">{preview.ipa}</span>
      )}
    </>
  );
}

const selectedEqual = (a: Focus | null, b: Focus) =>
  JSON.stringify(a) === JSON.stringify(b);
const Icon = ({ name }: { name: string }) => (
  <svg className="icon">
    <use href={`#icon-${name}`} />
  </svg>
);
const TableActionIcon = ({
  action,
}: {
  action:
    | "edit"
    | "row-above"
    | "row-below"
    | "column-left"
    | "column-right"
    | "split-row"
    | "split-column"
    | "delete"
    | "merge"
    | "unmerge";
}) => {
  if (action === "merge" || action === "unmerge")
    return (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="1em"
        height="1em"
        viewBox="0 0 24 24"
      >
        <g
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M4 6v12h16V6z" />
          {action === "merge" ? (
            <path d="M9 12h6M9 12l2-2m-2 2l2 2m4-2l-2-2m2 2l-2 2" />
          ) : (
            <>
              <path d="M12 7v10" />
              <path d="M9 12H6m0 0l1.5-1.5M6 12l1.5 1.5" />
              <path d="M15 12h3m0 0l-1.5-1.5M18 12l-1.5 1.5" />
            </>
          )}
        </g>
      </svg>
    );
  const paths = {
    edit: "M5 19h1.425L16.2 9.225L14.775 7.8L5 17.575zm-1 2q-.425 0-.712-.288T3 20v-2.425q0-.4.15-.763t.425-.637L16.2 3.575q.3-.275.663-.425t.762-.15t.775.15t.65.45L20.425 5q.3.275.437.65T21 6.4q0 .4-.138.763t-.437.662l-12.6 12.6q-.275.275-.638.425t-.762.15zM19 6.4L17.6 5zm-3.525 2.125l-.7-.725L16.2 9.225z",
    "row-above":
      "M5 14v6h14v-6zm0 8q-.825 0-1.412-.587T3 20V6q0-.825.588-1.412T5 4h1q.425 0 .713.288T7 5t-.288.713T6 6H5v6h14V6h-1q-.425 0-.712-.288T17 5t.288-.712T18 4h1q.825 0 1.413.588T21 6v14q0 .825-.587 1.413T19 22zm6-16h-1q-.425 0-.712-.288T9 5t.288-.712T10 4h1V3q0-.425.288-.712T12 2t.713.288T13 3v1h1q.425 0 .713.288T15 5t-.288.713T14 6h-1v1q0 .425-.288.713T12 8t-.712-.288T11 7zm1 8v-2zm0 0",
    "row-below":
      "M19 10V4H5v6zM5 20q-.825 0-1.412-.587T3 18V4q0-.825.588-1.412T5 2h14q.825 0 1.413.588T21 4v14q0 .825-.587 1.413T19 20h-1q-.425 0-.712-.288T17 19t.288-.712T18 18h1v-6H5v6h1q.425 0 .713.288T7 19t-.288.713T6 20zm7 2q-.425 0-.712-.288T11 21v-1h-1q-.425 0-.712-.288T9 19t.288-.712T10 18h1v-1q0-.425.288-.712T12 16t.713.288T13 17v1h1q.425 0 .713.288T15 19t-.288.713T14 20h-1v1q0 .425-.288.713T12 22m0-12v2zm0 0",
    "column-left":
      "M14 19h6V5h-6zm-8 2q-.825 0-1.412-.587T4 19v-1q0-.425.288-.712T5 17t.713.288T6 18v1h6V5H6v1q0 .425-.288.713T5 7t-.712-.288T4 6V5q0-.825.588-1.412T6 3h14q.825 0 1.413.588T22 5v14q0 .825-.587 1.413T20 21zm-1-6q-.425 0-.712-.288T4 14v-1H3q-.425 0-.712-.288T2 12t.288-.712T3 11h1v-1q0-.425.288-.712T5 9t.713.288T6 10v1h1q.425 0 .713.288T8 12t-.288.713T7 13H6v1q0 .425-.288.713T5 15m9-3h-2zm0 0",
    "column-right":
      "M10 5H4v14h6zM4 21q-.825 0-1.412-.587T2 19V5q0-.825.588-1.412T4 3h14q.825 0 1.413.588T20 5v1q0 .425-.288.713T19 7t-.712-.288T18 6V5h-6v14h6v-1q0-.425.288-.712T19 17t.713.288T20 18v1q0 .825-.587 1.413T18 21zm15-6q-.425 0-.712-.288T18 14v-1h-1q-.425 0-.712-.288T16 12t.288-.712T17 11h1v-1q0-.425.288-.712T19 9t.713.288T20 10v1h1q.425 0 .713.288T22 12t-.288.713T21 13h-1v1q0 .425-.288.713T19 15m-9-3h2zm0 0",
    "split-row":
      "M5 21q-.825 0-1.412-.587T3 19v-4q0-.825.588-1.412T5 13h14q.825 0 1.413.588T21 15v4q0 .825-.587 1.413T19 21zm0-10q-.825 0-1.412-.587T3 9V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v4q0 .825-.587 1.413T19 11zm14-6H5v4h14zM5 9V5z",
    "split-column":
      "M5 21q-.825 0-1.412-.587T3 19v-4q0-.825.588-1.412T5 13h14q.825 0 1.413.588T21 15v4q0 .825-.587 1.413T19 21zm0-10q-.825 0-1.412-.587T3 9V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v4q0 .825-.587 1.413T19 11zm14-6H5v4h14zM5 9V5z",
    delete:
      "M7 21q-.825 0-1.412-.587T5 19V6q-.425 0-.712-.288T4 5t.288-.712T5 4h4q0-.425.288-.712T10 3h4q.425 0 .713.288T15 4h4q.425 0 .713.288T20 5t-.288.713T19 6v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zm-7 11q.425 0 .713-.288T11 16V9q0-.425-.288-.712T10 8t-.712.288T9 9v7q0 .425.288.713T10 17m4 0q.425 0 .713-.288T15 16V9q0-.425-.288-.712T14 8t-.712.288T13 9v7q0 .425.288.713T14 17M7 6v13z",
  } as const;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="1em"
      height="1em"
      viewBox="0 0 24 24"
      style={
        action === "split-column" ? { transform: "rotate(90deg)" } : undefined
      }
    >
      <path fill="currentColor" d={paths[action]} />
    </svg>
  );
};

function ColumnHeaders({
  state,
  dispatch,
  openHeading,
}: {
  state: State<Cell>;
  dispatch: (action: Action<Cell>) => void;
  openHeading: (path: HeadingPath) => void;
}) {
  const depth = headingDepth(state.body.columns);
  const collect = (
    columns: Column[],
    target: number,
    current: number,
    prefix: HeadingPath = [],
  ): {
    column: Column;
    path: HeadingPath;
    colSpan: number;
    rowSpan: number;
  }[] =>
    columns.flatMap((column, index) => {
      const path = [...prefix, index];
      if (current === target)
        return [
          {
            column,
            path,
            colSpan: column.type === "Group" ? leafCount(column.columns) : 1,
            rowSpan: column.type === "Group" ? 1 : depth - current + 1,
          },
        ];
      return column.type === "Group"
        ? collect(column.columns, target, current + 1, path)
        : [];
    });
  return (
    <>
      {Array.from({ length: depth }, (_, row) => (
        <tr key={row}>
          {row === 0 && (
            <th
              className={`top-left ${state.focus.type === "TopLeft" ? "focused" : ""}`}
              rowSpan={depth}
              colSpan={headingDepth(state.body.rows)}
              tabIndex={state.focus.type === "TopLeft" ? 0 : -1}
              data-focus={serializeFocus({ type: "TopLeft" })}
              onClick={() =>
                dispatch({ type: "Focus", focus: { type: "TopLeft" } })
              }
            />
          )}
          {collect(state.body.columns, row + 1, 1).map(
            ({ column, path, colSpan, rowSpan }) => {
              const focus: Focus = { type: "ColumnHeading", path };
              const focused = selectedEqual(state.focus, focus);
              const selected = selectedEqual(state.select, focus);
              return (
                <th
                  key={path.join(",")}
                  colSpan={colSpan > 1 ? colSpan : undefined}
                  rowSpan={rowSpan > 1 ? rowSpan : undefined}
                  tabIndex={focused ? 0 : -1}
                  data-focus={serializeFocus(focus)}
                  className={`${focused ? "focused" : ""} ${selected ? "selected" : ""}`}
                  onClick={() => {
                    dispatch({ type: "Focus", focus });
                    dispatch({ type: "Select", select: focus });
                  }}
                  onDoubleClick={() => openHeading(path)}
                >
                  {column.heading}
                </th>
              );
            },
          )}
        </tr>
      ))}
    </>
  );
}

function GrammarRows({
  state,
  dispatch,
  preamble,
  example,
  hasIpaEstimator,
  runPreview,
  openCell,
  onPreviewChange,
  openHeading,
}: {
  state: State<Cell>;
  dispatch: (action: Action<Cell>) => void;
  preamble: string;
  example: PreviewExample | null;
  hasIpaEstimator: boolean;
  runPreview: PreviewRunner;
  openCell: (row: number, column: number) => void;
  onPreviewChange: (
    focus: Extract<Focus, { type: "Cell" }>,
    preview: Preview,
  ) => void;
  openHeading: (path: HeadingPath) => void;
}) {
  const rows = flatRows(state.body.rows);
  const covered = coveredCells(state.body.rows);
  return (
    <>
      {rows.map(({ path, row, headings }, rowIndex) => (
        <tr key={path.join(",")}>
          {headings.map((heading) => {
            const focus: Focus = { type: "RowHeading", path: heading.path };
            const focused = selectedEqual(state.focus, focus);
            const selected = selectedEqual(state.select, focus);
            return (
              <th
                key={heading.path.join(",")}
                rowSpan={heading.rowSpan > 1 ? heading.rowSpan : undefined}
                colSpan={heading.colSpan > 1 ? heading.colSpan : undefined}
                tabIndex={focused ? 0 : -1}
                data-focus={serializeFocus(focus)}
                className={`${focused ? "focused" : ""} ${selected ? "selected" : ""}`}
                onClick={() => {
                  dispatch({ type: "Focus", focus });
                  dispatch({ type: "Select", select: focus });
                }}
                onDoubleClick={() => openHeading(heading.path)}
              >
                {heading.heading}
              </th>
            );
          })}
          {row.cells.map((cell, column) =>
            covered.has(`${rowIndex},${column}`) ? null : (
              <CellView
                key={column}
                cell={cell}
                focus={{ type: "Cell", row: rowIndex, column }}
                state={state}
                dispatch={dispatch}
                preamble={preamble}
                example={example}
                hasIpaEstimator={hasIpaEstimator}
                runPreview={runPreview}
                openCell={openCell}
                onPreviewChange={onPreviewChange}
              />
            ),
          )}
        </tr>
      ))}
    </>
  );
}

function CellView({
  cell,
  focus,
  state,
  dispatch,
  preamble,
  example,
  hasIpaEstimator,
  runPreview,
  openCell,
  onPreviewChange,
}: {
  cell: Cell;
  focus: Extract<Focus, { type: "Cell" }>;
  state: State<Cell>;
  dispatch: (action: Action<Cell>) => void;
  preamble: string;
  example: PreviewExample | null;
  hasIpaEstimator: boolean;
  runPreview: PreviewRunner;
  openCell: (row: number, column: number) => void;
  onPreviewChange: (
    focus: Extract<Focus, { type: "Cell" }>,
    preview: Preview,
  ) => void;
}) {
  const focused = selectedEqual(state.focus, focus);
  const selected = selectedEqual(state.select, focus);
  const reportPreview = useCallback(
    (preview: Preview) => onPreviewChange(focus, preview),
    [focus.column, focus.row, onPreviewChange],
  );
  const rowFocused =
    state.focus.type === "Cell" && state.focus.row === focus.row;
  const columnFocused =
    state.focus.type === "Cell" && state.focus.column === focus.column;
  return (
    <td
      tabIndex={focused ? 0 : -1}
      data-focus={serializeFocus(focus)}
      rowSpan={cellRowspan(cell) > 1 ? cellRowspan(cell) : undefined}
      colSpan={cellColspan(cell) > 1 ? cellColspan(cell) : undefined}
      className={[
        "grammar-editor-cell",
        "cell",
        focused && "focused",
        selected && "selected",
        rowFocused && "row-focused",
        columnFocused && "col-focused",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={(event) => {
        dispatch({ type: "Focus", focus });
        if (!event.shiftKey) dispatch({ type: "Select", select: focus });
      }}
      onDoubleClick={() => openCell(focus.row, focus.column)}
      onKeyDown={(event) => {
        if (event.key === " ") {
          event.preventDefault();
          dispatch({ type: "Select", select: selected ? null : focus });
        }
      }}
    >
      <span className="grammar-preview-value">
        <PreviewCell
          cell={cell}
          preamble={preamble}
          example={example}
          hasIpaEstimator={hasIpaEstimator}
          runPreview={runPreview}
          onPreviewChange={reportPreview}
        />
      </span>
    </td>
  );
}

function Controls({
  state,
  dispatch,
  openCell,
  openHeading,
}: {
  state: State<Cell>;
  dispatch: (action: Action<Cell>) => void;
  openCell: () => void;
  openHeading: () => void;
}) {
  const selected = state.select;
  const kind =
    selected?.type === "RowHeading"
      ? "row"
      : selected?.type === "ColumnHeading"
        ? "column"
        : null;
  const canMerge =
    selected?.type === "Cell" &&
    state.focus.type === "Cell" &&
    !selectedEqual(selected, state.focus);
  const selectedCell =
    selected?.type === "Cell"
      ? flatRows(state.body.rows)[selected.row]?.row.cells[selected.column]
      : null;
  const Button = ({
    title,
    enabled,
    children,
    action,
  }: {
    title: string;
    enabled: boolean;
    children: React.ReactNode;
    action: () => void;
  }) => (
    <ControlButton title={title} enabled={enabled} onClick={action}>
      {children}
    </ControlButton>
  );
  const heading = (
    type: "AddHeading" | "DeleteHeading" | "SplitHeading",
    position?: "before" | "after",
  ) => {
    if (
      !kind ||
      !selected ||
      selected.type === "Cell" ||
      selected.type === "TopLeft"
    )
      return;
    if (type === "AddHeading")
      dispatch({ type, kind, path: selected.path, position: position! });
    else dispatch({ type, kind, path: selected.path });
  };
  return (
    <div className="controls-container grammar-editor-controls">
      <div className="controls">
        <span className="controls-header">History</span>
        <Button
          title="Undo"
          enabled={state.undoStack.length > 0}
          action={() => dispatch({ type: "Undo" })}
        >
          <Icon name="undo" />
        </Button>
        <Button
          title="Redo"
          enabled={state.redoStack.length > 0}
          action={() => dispatch({ type: "Redo" })}
        >
          <Icon name="redo" />
        </Button>
      </div>
      <div className="controls">
        <span className="controls-header">Row</span>
        <Button
          title="Edit row heading"
          enabled={kind === "row"}
          action={openHeading}
        >
          <TableActionIcon action="edit" />
        </Button>
        <Button
          title="Add row above"
          enabled={kind === "row"}
          action={() => heading("AddHeading", "before")}
        >
          <TableActionIcon action="row-above" />
        </Button>
        <Button
          title="Add row below"
          enabled={kind === "row"}
          action={() => heading("AddHeading", "after")}
        >
          <TableActionIcon action="row-below" />
        </Button>
        <Button
          title="Split row"
          enabled={kind === "row"}
          action={() => heading("SplitHeading")}
        >
          <TableActionIcon action="split-row" />
        </Button>
        <Button
          title="Delete row"
          enabled={kind === "row" && leafCount(state.body.rows) > 1}
          action={() => heading("DeleteHeading")}
        >
          <TableActionIcon action="delete" />
        </Button>
      </div>
      <div className="controls">
        <span className="controls-header">Column</span>
        <Button
          title="Edit column heading"
          enabled={kind === "column"}
          action={openHeading}
        >
          <TableActionIcon action="edit" />
        </Button>
        <Button
          title="Add column left"
          enabled={kind === "column"}
          action={() => heading("AddHeading", "before")}
        >
          <TableActionIcon action="column-left" />
        </Button>
        <Button
          title="Add column right"
          enabled={kind === "column"}
          action={() => heading("AddHeading", "after")}
        >
          <TableActionIcon action="column-right" />
        </Button>
        <Button
          title="Split column"
          enabled={kind === "column"}
          action={() => heading("SplitHeading")}
        >
          <TableActionIcon action="split-column" />
        </Button>
        <Button
          title="Delete column"
          enabled={kind === "column" && leafCount(state.body.columns) > 1}
          action={() => heading("DeleteHeading")}
        >
          <TableActionIcon action="delete" />
        </Button>
      </div>
      <div className="controls">
        <span className="controls-header">Cell</span>
        <Button
          title="Edit cell sound changes"
          enabled={selected?.type === "Cell"}
          action={openCell}
        >
          <TableActionIcon action="edit" />
        </Button>
        <Button
          title="Merge selected cell with focused cell"
          enabled={!!canMerge}
          action={() =>
            selected?.type === "Cell" &&
            state.focus.type === "Cell" &&
            dispatch({ type: "Merge", first: selected, second: state.focus })
          }
        >
          <TableActionIcon action="merge" />
        </Button>
        <Button
          title="Unmerge cell"
          enabled={
            !!selectedCell &&
            (cellRowspan(selectedCell) > 1 || cellColspan(selectedCell) > 1)
          }
          action={() =>
            selected?.type === "Cell" &&
            dispatch({
              type: "Unmerge",
              row: selected.row,
              column: selected.column,
            })
          }
        >
          <TableActionIcon action="unmerge" />
        </Button>
      </div>
    </div>
  );
}

/// Shows the IPA of the example's unchanged form. Cell previews estimate IPA
/// after their inflection rules have run, so this is only a base-word hint.
function EstimatedInput({
  example,
  runPreview,
}: {
  example: PreviewExample | null;
  runPreview: PreviewRunner;
}) {
  const [preview, setPreview] = useState<Preview>({ kind: "empty" });
  useEffect(() => {
    if (!example?.word.trim()) {
      setPreview({ kind: "empty" });
      return;
    }
    let cancelled = false;
    setPreview({ kind: "running" });
    const timer = window.setTimeout(() => {
      void runPreview({ ...example, word: example.word.trim() }, "", "")
        .then((result) => !cancelled && setPreview(result))
        .catch((error) => {
          if (!cancelled)
            setPreview({
              kind: "error",
              message:
                error instanceof Error
                  ? error.message
                  : "could not estimate IPA",
            });
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [example, runPreview]);
  if (preview.kind === "empty") return null;
  if (preview.kind === "running")
    return <p className="hint">estimating IPA…</p>;
  if (preview.kind === "timed_out")
    return <p className="grammar-preview-error">IPA estimation timed out</p>;
  if (preview.kind === "error")
    return <p className="grammar-preview-error">{preview.message}</p>;
  if (!preview.ipa) return null;
  return (
    <p className="hint">
      estimated IPA: <code>{preview.ipa}</code>
    </p>
  );
}

function GrammarEditor({
  body,
  initialPreamble,
  previewUrl,
  hasIpaEstimator,
  languageCode,
  name,
}: {
  body: GrammarBody;
  initialPreamble: string;
  previewUrl: string;
  hasIpaEstimator: boolean;
  languageCode: string;
  name: string;
}) {
  const [state, setState] = useState(() => initialState(body));
  const [preamble, setPreamble] = useState(initialPreamble);
  const [example, setExample] = useState<PreviewExample | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const [cellModal, setCellModal] = useState<Extract<
    Focus,
    { type: "Cell" }
  > | null>(null);
  const [cellPreviewErrors, setCellPreviewErrors] = useState(
    new Map<string, string>(),
  );
  const [headingModal, setHeadingModal] = useState<{
    kind: "row" | "column";
    path: HeadingPath;
  } | null>(null);
  const table = useRef<HTMLTableElement>(null);
  const runPreview = usePreviewRunner(previewUrl);
  const preambleDeclarations = useMemo(
    () => declaredLexurgyNames(preamble),
    [preamble],
  );
  const dispatch = (action: Action<Cell>) =>
    setState((old) => apply(old, action, options));
  const updatePreviewError = useCallback(
    (focus: Extract<Focus, { type: "Cell" }>, preview: Preview) => {
      const key = `${focus.row},${focus.column}`;
      const message = preview.kind === "error" ? preview.message : undefined;
      setCellPreviewErrors((previous) => {
        if (previous.get(key) === message) return previous;
        const next = new Map(previous);
        if (message) next.set(key, message);
        else next.delete(key);
        return next;
      });
    },
    [],
  );
  const selectedCell = cellModal
    ? flatRows(state.body.rows)[cellModal.row]?.row.cells[cellModal.column]
    : null;
  const selectedCellPreviewError = cellModal
    ? cellPreviewErrors.get(`${cellModal.row},${cellModal.column}`)
    : null;
  const selectedHeading = (
    headingModal
      ? headingAt(
          (headingModal.kind === "row"
            ? state.body.rows
            : state.body.columns) as any[],
          headingModal.path,
        )
      : null
  ) as { heading: string } | null;
  const onKeyDown = (event: React.KeyboardEvent) => {
    const movement = movementFromKey(event.nativeEvent);
    if (movement) {
      const next = moveFocus(state.body, state.focus, movement);
      if (next !== null) {
        event.preventDefault();
        dispatch({ type: "Focus", focus: next });
      }
    } else if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === "z"
    ) {
      event.preventDefault();
      dispatch({ type: event.shiftKey ? "Redo" : "Undo" });
    } else if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === "y"
    ) {
      event.preventDefault();
      dispatch({ type: "Redo" });
    } else if (event.key === " ") {
      event.preventDefault();
      dispatch({ type: "Select", select: state.select ? null : state.focus });
    } else if (
      event.key === "m" &&
      state.select?.type === "Cell" &&
      state.focus.type === "Cell"
    )
      dispatch(
        event.shiftKey
          ? {
              type: "Unmerge",
              row: state.select.row,
              column: state.select.column,
            }
          : { type: "Merge", first: state.select, second: state.focus },
      );
    else if (event.key === "Enter" && state.focus.type === "Cell")
      setCellModal(state.focus);
  };
  useEffect(() => {
    const selector = `[data-focus="${serializeFocus(state.focus).replaceAll(/"/g, '\\"')}"]`;
    (table.current?.querySelector(selector) as HTMLElement | null)?.focus();
  }, [state.focus]);
  const hiddenBody = useMemo(() => JSON.stringify(state.body), [state.body]);
  return (
    <div className="grammar-editor phonology-editor">
      <input type="hidden" name="body" value={hiddenBody} />
      <input type="hidden" name="preamble" value={preamble} />
      <Controls
        state={state}
        dispatch={dispatch}
        openCell={() =>
          state.select?.type === "Cell" && setCellModal(state.select)
        }
        openHeading={() =>
          state.select?.type === "RowHeading"
            ? setHeadingModal({ kind: "row", path: state.select.path })
            : state.select?.type === "ColumnHeading" &&
              setHeadingModal({ kind: "column", path: state.select.path })
        }
      />
      <div className="grammar-table-scroll">
        <div className="header-with-actions">
          <h2>{name}</h2>
          <ul>
            <li>
              <Help open={helpOpen} setOpen={setHelpOpen} editor="grammar" />
            </li>
          </ul>
        </div>
        <table
          ref={table}
          className="grammar-table focus-inside-table"
          onKeyDown={onKeyDown}
        >
          <thead>
            <ColumnHeaders
              state={state}
              dispatch={dispatch}
              openHeading={(path) => setHeadingModal({ kind: "column", path })}
            />
          </thead>
          <tbody>
            <GrammarRows
              state={state}
              dispatch={dispatch}
              preamble={preamble}
              example={example}
              hasIpaEstimator={hasIpaEstimator}
              runPreview={runPreview}
              openCell={(row, column) =>
                setCellModal({ type: "Cell", row, column })
              }
              onPreviewChange={updatePreviewError}
              openHeading={(path) => setHeadingModal({ kind: "row", path })}
            />
          </tbody>
        </table>
      </div>
      <section className="grammar-editor-example">
        <label htmlFor="grammar-example-word">Example word</label>
        <WordCombobox
          inputId="grammar-example-word"
          inputName="example_word"
          languageFilter={languageCode}
          onChange={setExample}
        />
        {hasIpaEstimator && (
          <EstimatedInput example={example} runPreview={runPreview} />
        )}
      </section>
      <section>
        <label>Shared sound changes (preamble)</label>
        <SoundChangeEditor
          value={preamble}
          onChange={setPreamble}
          label="Shared sound changes"
        />
      </section>
      <ModalInner
        open={cellModal !== null}
        close={() => setCellModal(null)}
        title="Cell sound changes"
        contents={(close) => (
          <>
            {selectedCellPreviewError && (
              <p className="grammar-cell-preview-error" role="alert">
                Preview error: {selectedCellPreviewError}
              </p>
            )}
            {selectedCell && (
              <SoundChangeEditor
                value={selectedCell.changes}
                onChange={(changes) =>
                  cellModal &&
                  dispatch({
                    type: "SetCell",
                    row: cellModal.row,
                    column: cellModal.column,
                    cell: { ...selectedCell, changes },
                  })
                }
                label="Cell sound changes"
                externalDeclarations={preambleDeclarations}
              />
            )}
            <p className="hint">
              This is the sound-change runner’s editor, including its Lexurgy
              highlighting and completion. Classes declared in the shared
              preamble are available to every cell.
            </p>
            <div className="button-row">
              <button type="button" className="normal" onClick={close}>
                Done
              </button>
            </div>
          </>
        )}
      />
      <ModalInner
        open={headingModal !== null}
        close={() => setHeadingModal(null)}
        title={`Edit ${headingModal?.kind ?? ""} heading`}
        contents={(close) => (
          <HeadingModal
            key={headingModal?.path.join(",")}
            heading={selectedHeading?.heading ?? ""}
            onSave={(heading) => {
              if (headingModal)
                dispatch({
                  type: "EditHeading",
                  kind: headingModal.kind,
                  path: headingModal.path,
                  heading,
                });
              close();
            }}
          />
        )}
      />
    </div>
  );
}

function HeadingModal({
  heading,
  onSave,
}: {
  heading: string;
  onSave: (heading: string) => void;
}) {
  const [value, setValue] = useState(heading);
  return (
    <>
      <section>
        <label htmlFor="grammar-heading">Name</label>
        <input
          id="grammar-heading"
          className="normal"
          type="text"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          autoFocus
        />
      </section>
      <div className="button-row">
        <button
          type="button"
          className="normal"
          disabled={!value.trim()}
          onClick={() => onSave(value)}
        >
          Save
        </button>
      </div>
    </>
  );
}

function mount() {
  const host = document.getElementById("table-editor");
  const bodyNode = document.getElementById("initial-grammar-table-body");
  const preambleNode = document.getElementById(
    "initial-grammar-table-preamble",
  );
  const optionsNode = document.getElementById("grammar-table-editor-options");
  if (!host || !bodyNode || !preambleNode || !optionsNode) return;
  const body = JSON.parse(bodyNode.textContent || "{}") as GrammarBody;
  const initialPreamble = JSON.parse(
    preambleNode.textContent || '""',
  ) as string;
  const options = JSON.parse(optionsNode.textContent || "{}") as {
    previewUrl: string;
    hasIpaEstimator?: boolean;
    languageCode: string;
    name?: string;
  };
  host.replaceWith(
    Object.assign(document.createElement("div"), {
      id: "grammar-table-editor",
    }),
  );
  ReactDOM.createRoot(document.getElementById("grammar-table-editor")!).render(
    <GrammarEditor
      body={body}
      initialPreamble={initialPreamble}
      previewUrl={options.previewUrl}
      hasIpaEstimator={options.hasIpaEstimator ?? false}
      languageCode={options.languageCode}
      name={options.name || "grammar table"}
    />,
  );
}
if (typeof window !== "undefined")
  window.addEventListener("DOMContentLoaded", mount);
