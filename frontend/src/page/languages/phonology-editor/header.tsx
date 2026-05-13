import type { HeadingPath } from "./path";
import { serializePath } from "./path";
import { isFocused, isSelected, useEditor } from "./state";
import { maxHeadingDepth, numLeaves, type Column } from "./table";

type ThCell = {
  heading: string;
  rowSpan: number;
  colSpan: number;
  path: HeadingPath;
};

const collectAtDepth = (
  columns: Column[],
  targetDepth: number,
  currentDepth: number,
  maxDepth: number,
  path: HeadingPath = [],
): ThCell[] => {
  const result: ThCell[] = [];
  for (let i = 0; i < columns.length; i++) {
    const col = columns[i]!;
    const colPath = [...path, i];
    if (currentDepth === targetDepth) {
      if (col.type === "Group") {
        result.push({
          heading: col.heading,
          colSpan: numLeaves(col.columns),
          rowSpan: 1,
          path: colPath,
        });
      } else {
        result.push({
          heading: col.heading,
          colSpan: 1,
          rowSpan: maxDepth - currentDepth + 1,
          path: colPath,
        });
      }
    } else if (col.type === "Group") {
      result.push(
        ...collectAtDepth(
          col.columns,
          targetDepth,
          currentDepth + 1,
          maxDepth,
          colPath,
        ),
      );
    }
  }
  return result;
};

const ColumnTh = ({ heading, rowSpan, colSpan, path }: ThCell) => {
  const [state, dispatch] = useEditor();
  const focused = isFocused(state, { type: "ColumnHeading", path });
  const selected = isSelected(state, { type: "ColumnHeading", path });

  const onClick = () => {
    if (!focused) {
      dispatch({ type: "SetFocus", path: { type: "ColumnHeading", path } });
      dispatch({ type: "SetSelect", path: { type: "ColumnHeading", path } });
    }
  };

  const onDoubleClick = () => {
    dispatch({ type: "SetFocus", path: { type: "ColumnHeading", path } });
    dispatch({ type: "SetSelect", path: { type: "ColumnHeading", path } });
    dispatch({ type: "OpenModal", modal: "EditColumnHeading" });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === " ") {
      e.preventDefault();
      if (focused) {
        dispatch({ type: "SetSelect", path: null });
      } else {
        dispatch({ type: "SetSelect", path: { type: "ColumnHeading", path } });
      }
    }
  };

  let rs: number | undefined = rowSpan > 1 ? rowSpan : undefined;
  let cs: number | undefined = colSpan > 1 ? colSpan : undefined;

  let className = focused ? "focused" : "";
  if (selected) {
    className += " selected";
  }

  const tabIndex = focused ? 0 : -1;

  return (
    <th
      rowSpan={rs}
      colSpan={cs}
      className={className}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onKeyDown={onKeyDown}
      tabIndex={tabIndex}
      data-path={serializePath(state.body, { type: "ColumnHeading", path })}
    >
      {heading}
    </th>
  );
};

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

  let colSpan: number | undefined = maxHeadingDepth(state.body.rows);
  colSpan = colSpan > 1 ? colSpan : undefined;

  let rowSpan: number | undefined = maxHeadingDepth(state.body.columns);
  rowSpan = rowSpan > 1 ? rowSpan : undefined;

  let className = "cell top-left";
  if (focused) {
    className += " focused";
  }

  const tabIndex = focused ? 0 : -1;

  return (
    <th
      className={className}
      rowSpan={rowSpan}
      colSpan={colSpan}
      tabIndex={tabIndex}
      data-path={serializePath(state.body, { type: "TopLeft" })}
    ></th>
  );
};
