import React, { useRef } from "react";
import { getMovement, isPathEqual, move, serializePath, type CellPath, type HeadingPath } from "./path";
import { isFocused, isSelected, useEditor } from "./state";
import { maxHeadingDepth, numLeaves, type Cell, type Row } from "./table";

const Phoneme = ({ text, annotations, isLast, onDoubleClick }: { text: string; annotations: number[]; isLast: boolean; onDoubleClick?: () => void }) => {
    const handleDoubleClick = onDoubleClick ? (e: React.MouseEvent) => { e.stopPropagation(); onDoubleClick(); } : undefined;
    return (
        <>
          <span className="phoneme" onDoubleClick={handleDoubleClick}>
              {text}
          </span>
          {annotations.map((index, i) => <><sup key={index} className="annotation-link">{index + 1}{ i === annotations.length - 1 ? "" : ", "}</sup></>)}
          {!isLast && <span className="phoneme-separator">, </span>}
        </>
    );
}

const RowCell = ({ cell, path, rowFocused }: { cell: Cell; path: CellPath; rowFocused: boolean }) => {
    const [state, dispatch] = useEditor();
    const focused = isFocused(state, path);
    const colFocused = isFocused(state, { type: "ColumnHeading", path: path.colPath });
    const selected = isSelected(state, path);
    const cellRef = useRef<HTMLTableCellElement>(null);

    const onClick = (e: React.MouseEvent) => {
      if (!focused) {
        dispatch({ type: "SetFocus", path });
        dispatch({ type: "SetSelect", path });
      }
    }

    const onDoubleClick = () => {
      dispatch({ type: "SetFocus", path });
      dispatch({ type: "SetSelect", path });
      if (cell.phonemes.length === 0) {
        dispatch({ type: "OpenModal", modal: "AddPhoneme" });
      }
    }

    const onKeyDown = (e: React.KeyboardEvent) => {
      if (e.key === " ") {
        e.preventDefault();
        if (selected) {
          dispatch({ type: "SetSelect", path: null });
        } else {
          dispatch({ type: "SetSelect", path });
        }
      }
    }

    let className = "cell";
    if (focused) {
      className += " focused";
    }
    if (colFocused) {
      className += " col-focused";
    }
    if (rowFocused) {
      className += " row-focused";
    }
    if (selected) {
      className += " selected";
    }

    const tabIndex = focused ? 0 : -1;

    return (
      <td ref={cellRef} className={className} onClick={onClick} onDoubleClick={onDoubleClick} tabIndex={tabIndex} data-path={serializePath(state.body, path)} onKeyDown={onKeyDown}>
        {cell.phonemes.map((p, i) => (
          <Phoneme key={p.text + i} text={p.text} annotations={p.annotations} isLast={i === cell.phonemes.length - 1} onDoubleClick={() => {
            dispatch({ type: "SetFocus", path });
            dispatch({ type: "SetSelect", path });
            dispatch({ type: "OpenModal", modal: "EditPhoneme", phonemeIndex: i });
          }} />
        ))}
      </td>
    )
}

type ThCell = { heading: string; rowSpan: number; colSpan: number; path: HeadingPath };
type FlatRow = { thCells: ThCell[]; leaf: { heading: string; cells: Cell[] }; path: HeadingPath };

const flattenRows = (rows: Row[], path: HeadingPath, depth: number, maxDepth: number): FlatRow[] => {
  const result: FlatRow[] = [];
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i]!;
    const rowPath = [...path, i];
    if (row.type === "Group") {
      const children = flattenRows(row.rows, rowPath, depth + 1, maxDepth);
      children[0]!.thCells.unshift({ heading: row.heading, rowSpan: numLeaves(row.rows), colSpan: 1, path: rowPath });
      result.push(...children);
    } else {
      result.push({
        thCells: [{ heading: row.heading, rowSpan: 1, colSpan: maxDepth - depth + 1, path: rowPath }],
        leaf: row,
        path: rowPath,
      });
    }
  }
  return result;
};

const Th = ({ heading, rowSpan, colSpan, path }: ThCell) => {
  const [state, dispatch] = useEditor();
  const focused = isFocused(state, { type: "RowHeading", path });
  const selected = isSelected(state, { type: "RowHeading", path });

  const onClick = (e: React.MouseEvent) => {
    if (!focused) {
      dispatch({ type: "SetFocus", path: { type: "RowHeading", path } });
      dispatch({ type: "SetSelect", path: { type: "RowHeading", path } });
    }
  }

  const onDoubleClick = () => {
    dispatch({ type: "SetFocus", path: { type: "RowHeading", path } });
    dispatch({ type: "SetSelect", path: { type: "RowHeading", path } });
    dispatch({ type: "OpenModal", modal: "EditRowHeading" });
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === " ") {
      e.preventDefault();
      if (selected) {
        dispatch({ type: "SetSelect", path: null });
      } else {
        dispatch({ type: "SetSelect", path: { type: "RowHeading", path } });
      }
    }
  }

  
  let rs: number | undefined = rowSpan > 1 ? rowSpan : undefined;
  let cs: number | undefined = colSpan > 1 ? colSpan : undefined;

  const tabIndex = focused ? 0 : -1;

  let className = focused ? "focused" : "";
  if (selected) {
    className += " selected";
  }

  return (
    <th rowSpan={rs} colSpan={cs} className={className} onClick={onClick} onDoubleClick={onDoubleClick} onKeyDown={onKeyDown} tabIndex={tabIndex} data-path={serializePath(state.body, { type: "RowHeading", path })}>
      {heading}
    </th>
  );
}

export const TableRows = () => {
  const [state] = useEditor();
  const { rows } = state.body;
  const maxDepth = maxHeadingDepth(rows);
  const flat = flattenRows(rows, [], 1, maxDepth);
  return (
    <>
      {flat.map((flatRow, i) => {
        const rowFocused = isFocused(state, { type: "RowHeading", path: flatRow.path });
        return (
          <tr key={i}>
            {flatRow.thCells.map((th, j) => (
              <Th key={j} {...th} />
            ))}
            {flatRow.leaf.cells.map((cell, j) => (
              <RowCell
                key={j}
                cell={cell}
                path={{ type: "Cell", rowPath: flatRow.path, colPath: j }}
                rowFocused={rowFocused}
              />
            ))}
          </tr>
        );
      })}
    </>
  );
};