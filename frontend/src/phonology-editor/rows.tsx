import { useRef } from "react";
import { getMovement, isPathEqual, move, type CellPath, type HeadingPath } from "./path";
import { isFocused, isSelected, useEditor } from "./state";
import { maxHeadingDepth, numLeaves, type Cell, type Row } from "./table";

const RowCell = ({ cell, path, rowFocused }: { cell: Cell; path: CellPath; rowFocused: boolean }) => {
    const [state, dispatch] = useEditor();
    const focused = isFocused(state, path);
    const colFocused = isFocused(state, { type: "ColumnHeading", path: path.colPath });
    const selected = isSelected(state, path);
    const cellRef = useRef<HTMLTableCellElement>(null);

    if(focused) {
      console.log("Focused cell", path);
    }

    const handleKeyPress = (e: React.KeyboardEvent) => {
      let movement = getMovement(e);
      if (movement) {
        const newFocus = move(state.body, state.focus, movement);
        console.log(newFocus);
        if (newFocus) {
          e.preventDefault();
          dispatch({ type: "SetFocus", path: newFocus });
        } else {
          cellRef.current?.blur();
        }
      }
    }

    const onClick = () => {
      if (!focused) {
        cellRef.current?.focus();
        dispatch({ type: "SetFocus", path });
      }
    }

    let className = "cell";
    if (focused && state.focusInsideTable) {
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
      <td ref={cellRef} className={className} onClick={onClick} onKeyDown={handleKeyPress} tabIndex={tabIndex}>
        {cell.phonemes.map(p => p.text).join(", ")}
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

  const onClick = () => {
    if (!focused) {
      dispatch({ type: "SetFocus", path: { type: "RowHeading", path } });
    }
  }
  
  let rs: number | undefined = rowSpan > 1 ? rowSpan : undefined;
  let cs: number | undefined = colSpan > 1 ? colSpan : undefined;

  return (
    <th rowSpan={rs} colSpan={cs} className={focused ? "focused" : ""} onClick={onClick}>
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