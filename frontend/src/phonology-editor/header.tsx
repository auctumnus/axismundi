import type { HeadingPath } from "./path";
import { isFocused, useEditor } from "./state";
import { maxHeadingDepth, numLeaves, type Column } from "./table";

type ThCell = { heading: string; rowSpan: number; colSpan: number; path: HeadingPath };

const collectAtDepth = (columns: Column[], targetDepth: number, currentDepth: number, maxDepth: number, path: HeadingPath = []): ThCell[] => {
  const result: ThCell[] = [];
  for (let i = 0; i < columns.length; i++) {
    const col = columns[i]!;
    const colPath = [...path, i];
    if (currentDepth === targetDepth) {
      if (col.type === "Group") {
        result.push({ heading: col.heading, colSpan: numLeaves(col.columns), rowSpan: 1, path: colPath });
      } else {
        result.push({ heading: col.heading, colSpan: 1, rowSpan: maxDepth - currentDepth + 1, path: colPath });
      }
    } else if (col.type === "Group") {
      result.push(...collectAtDepth(col.columns, targetDepth, currentDepth + 1, maxDepth, colPath));
    }
  }
  return result;
};

const ColumnTh = ({ heading, rowSpan, colSpan, path }: ThCell) => {
  const [state, dispatch] = useEditor();
  const focused = isFocused(state, { type: "ColumnHeading", path });

  const onClick = () => {
    if (!focused) {
      dispatch({ type: "SetFocus", path: { type: "ColumnHeading", path } });
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

export const ColumnHeaders = () => {
  const [state] = useEditor();
  const { columns } = state.body;
  const maxDepth = maxHeadingDepth(columns);
  return (
    <>
      {Array.from({ length: maxDepth }, (_, i) => i + 1).map((depth, i) => (
        <tr key={depth}>
          {i === 0 && <TopLeftCell />}
          {collectAtDepth(columns, depth, 1, maxDepth).map((th, j) => (
            <ColumnTh key={j} {...th} />
          ))}
        </tr>
      ))}
    </>
  );
};

export const TopLeftCell = () => {
  const [state, dispatch] = useEditor();
  const focused = isFocused(state, { type: "TopLeft" });

  let colSpan: number | undefined = maxHeadingDepth(state.body.columns);
  colSpan = colSpan > 1 ? colSpan : undefined;

  let rowSpan: number | undefined = maxHeadingDepth(state.body.rows);
  rowSpan = rowSpan > 1 ? rowSpan : undefined;

  let className = "cell top-left";
  if (focused) {
    className += " focused";
  }

  const onClick = () => {
    if (!focused) {
      dispatch({ type: "SetFocus", path: { type: "TopLeft" } });
    }
  }

  return (
    <th className={className} rowSpan={rowSpan} colSpan={colSpan} onClick={onClick}> 
    </th>
  );
}

