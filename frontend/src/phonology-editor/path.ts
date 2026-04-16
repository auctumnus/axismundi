import type { Body, Cell, Column, Row, Heading, TableElement } from "./table";
import {
  elementKind,
  headingChildren,
  numLeaves,
  TOP_LEFT_CELL,
} from "./table";

export type HeadingPath = number[];

export interface TopLeftPath {
  type: "TopLeft";
}

// in paths, if a number is used to refer to a heading, it is the index of that heading
// among its siblings; however, this will only ever refer to leaf headings (i.e. individual
// rows or columns)

export interface RowHeadingPath {
  type: "RowHeading";
  path: HeadingPath | number;
}

export interface ColumnHeadingPath {
  type: "ColumnHeading";
  path: HeadingPath | number;
}

export interface CellPath {
  type: "Cell";
  rowPath: HeadingPath | number;
  colPath: HeadingPath | number;
}

export type TablePath =
  | TopLeftPath
  | RowHeadingPath
  | ColumnHeadingPath
  | CellPath;

export type PathTarget<T extends TablePath> = T extends TopLeftPath
  ? typeof TOP_LEFT_CELL
  : T extends RowHeadingPath
    ? Row
    : T extends ColumnHeadingPath
      ? Column
      : T extends CellPath
        ? Cell
        : TableElement;

export const countLeaves = (heading: Heading): number => {
  const children = headingChildren(heading);
  if (children.length === 0) {
    return 1;
  }
  return children.reduce((sum, child) => sum + countLeaves(child), 0);
};

/**
 * Returns the overall index of the heading at the end of the given path, or null if the path is invalid.
 * The overall index is the index of the heading among all leaf headings in a left-to-right, top-to-bottom traversal of the table.
 * @param headings
 * @param path
 */
export const headingPathToIndex = (
  headings: Heading[],
  path: HeadingPath,
): number | null => {
  let index = 0;
  let currentHeadings = headings;
  for (const pathIndex of path) {
    if (pathIndex >= currentHeadings.length) {
      return null;
    }
    const currentHeading = currentHeadings[pathIndex]!;
    index += currentHeadings
      .slice(0, pathIndex)
      .reduce((sum, heading) => sum + countLeaves(heading), 0);
    currentHeadings = headingChildren(currentHeading);
  }
  return index;
};

export const indexToHeadingPath = (
  headings: Heading[],
  targetIndex: number,
): HeadingPath | null => {
  let index = 0;
  let path: number[] = [];
  let currentHeadings = headings;
  while (true) {
    let found = false;
    for (let i = 0; i < currentHeadings.length; i++) {
      const heading = currentHeadings[i]!;
      const numLeaves = countLeaves(heading);
      if (index + numLeaves > targetIndex) {
        path.push(i);
        currentHeadings = headingChildren(heading);
        found = true;
        break;
      }
      index += numLeaves;
    }
    if (!found) {
      return null;
    }
    if (currentHeadings.length === 0) {
      return path;
    }
  }
};

const stepHeadingPath = <T extends Row | Column>(
  current: T,
  index: number,
): T | null => {
  if (current.type === "Group") {
    const children = headingChildren(current);
    return (children[index] as T) ?? null;
  }
  return null;
};

const followHeadingPath = <T extends Row | Column>(
  headings: T[],
  path: HeadingPath,
): T | null => {
  let firstIndex = path[0];
  if (firstIndex === undefined) {
    return null;
  }
  if (headings[firstIndex] === undefined) {
    return null;
  }
  let current: T = headings[firstIndex];
  for (const index of path.slice(1)) {
    let next = stepHeadingPath(current, index);
    if (next === null) {
      return null;
    }
    current = next;
  }
  return current;
};

export const normalizeHeadingPath = (
  headings: Heading[],
  path: HeadingPath | number,
): HeadingPath | null => {
  if (typeof path === "number") {
    const headingPath = indexToHeadingPath(headings, path);
    if (headingPath === null) {
      return null;
    }
    return headingPath;
  }
  return path;
};

const normalizeToIndex = (
  headings: Heading[],
  path: HeadingPath | number,
): number | null => {
  if (typeof path === "number") {
    return path;
  }
  const index = headingPathToIndex(headings, path);
  if (index === null) {
    return null;
  }
  return index;
};

export const isPathPrefixed = (
  prefix: HeadingPath,
  path: HeadingPath,
): boolean => {
  if (prefix.length > path.length) {
    return false;
  }
  for (let i = 0; i < prefix.length; i++) {
    if (prefix[i] !== path[i]) {
      return false;
    }
  }
  return true;
};

export const isPathEqual = (
  table: Body,
  path1: TablePath,
  path2: TablePath,
): boolean => {
  if (path1.type === path2.type) {
    switch (path1.type) {
      case "TopLeft":
        return true;
      case "RowHeading": {
        const normalizedPath1 = normalizeHeadingPath(table.rows, path1.path);
        const normalizedPath2 = normalizeHeadingPath(
          table.rows,
          (path2 as RowHeadingPath).path,
        );
        if (normalizedPath1 === null || normalizedPath2 === null) {
          return false;
        }
        return (
          JSON.stringify(normalizedPath1) === JSON.stringify(normalizedPath2)
        );
      }
      case "ColumnHeading": {
        const normalizedColPath1 = normalizeHeadingPath(
          table.columns,
          path1.path,
        );
        const normalizedColPath2 = normalizeHeadingPath(
          table.columns,
          (path2 as ColumnHeadingPath).path,
        );
        if (normalizedColPath1 === null || normalizedColPath2 === null) {
          return false;
        }
        return (
          JSON.stringify(normalizedColPath1) ===
          JSON.stringify(normalizedColPath2)
        );
      }
      case "Cell":
        const normalizedRowPath1 = normalizeToIndex(table.rows, path1.rowPath);
        const normalizedRowPath2 = normalizeToIndex(
          table.rows,
          (path2 as CellPath).rowPath,
        );
        const normalizedColPath1 = normalizeToIndex(
          table.columns,
          path1.colPath,
        );
        const normalizedColPath2 = normalizeToIndex(
          table.columns,
          (path2 as CellPath).colPath,
        );
        if (
          normalizedRowPath1 === null ||
          normalizedRowPath2 === null ||
          normalizedColPath1 === null ||
          normalizedColPath2 === null
        ) {
          return false;
        }
        const areRowsEqual = normalizedRowPath1 === normalizedRowPath2;
        const areColsEqual = normalizedColPath1 === normalizedColPath2;
        return areRowsEqual && areColsEqual;
    }
  }
  return false;
};

/**
 * Gets the table element identified by the given path, or null if the path is invalid.
 * @param table
 * @param path
 * @returns
 */
export const getByPath = <T extends TablePath>(
  table: Body,
  path: T,
): PathTarget<T> | null => {
  switch (path.type) {
    case "TopLeft":
      return TOP_LEFT_CELL as PathTarget<T>;
    case "RowHeading": {
      const normalizedPath = normalizeHeadingPath(table.rows, path.path);
      if (normalizedPath === null) {
        return null;
      }
      return followHeadingPath(table.rows, normalizedPath) as PathTarget<T>;
    }
    case "ColumnHeading": {
      const normalizedPath = normalizeHeadingPath(table.columns, path.path);
      if (normalizedPath === null) {
        return null;
      }
      return followHeadingPath(table.columns, normalizedPath) as PathTarget<T>;
    }
    case "Cell": {
      const normalizedRowPath = normalizeHeadingPath(table.rows, path.rowPath);
      if (normalizedRowPath === null) {
        return null;
      }
      const rowHeading = followHeadingPath(table.rows, normalizedRowPath);
      let indexInRow: number;
      if (typeof path.colPath === "number") {
        indexInRow = path.colPath;
      } else {
        const i = headingPathToIndex(table.columns, path.colPath);
        if (i === null) {
          return null;
        }
        indexInRow = i;
      }
      if (rowHeading === null || indexInRow === null) {
        return null;
      }
      if (rowHeading.type !== "Individual") {
        return null;
      }
      const cell = rowHeading.cells[indexInRow];
      if (cell === undefined) {
        return null;
      }
      return cell as PathTarget<T>;
    }
  }
};

const setByPathInHeadings = <T extends Row | Column>(
  headings: T[],
  path: HeadingPath,
  newValue: Partial<T>,
): T[] => {
  const [index, ...rest] = path;
  if (index === undefined) {
    return headings;
  }
  return headings.map((heading, i) => {
    if (i !== index) return heading;
    if (rest.length === 0) {
      return { ...heading, ...newValue };
    }
    // recurse into children
    if (heading.type === "Group") {
      if ("rows" in heading) {
        return {
          ...heading,
          rows: setByPathInHeadings(heading.rows as T[], rest, newValue),
        };
      } else {
        return {
          ...heading,
          columns: setByPathInHeadings(
            (heading as any).columns as T[],
            rest,
            newValue,
          ),
        };
      }
    }
    return heading;
  });
};

const setCellInRow = (
  rows: Row[],
  rowPath: HeadingPath,
  colIndex: number,
  newValue: Partial<Cell>,
): Row[] => {
  const [index, ...rest] = rowPath;
  if (index === undefined) {
    return rows;
  }
  return rows.map((row, i) => {
    if (i !== index) return row;
    if (rest.length === 0) {
      if (row.type !== "Individual") return row;
      return {
        ...row,
        cells: row.cells.map((cell, ci) =>
          ci === colIndex ? { ...cell, ...newValue } : cell,
        ),
      };
    }
    if (row.type === "Group") {
      return { ...row, rows: setCellInRow(row.rows, rest, colIndex, newValue) };
    }
    return row;
  });
};

export const setByPath = <T extends TablePath>(
  table: Body,
  path: T,
  newValue: Partial<PathTarget<T>>,
): Body => {
  switch (path.type) {
    case "TopLeft":
      return table;
    case "RowHeading": {
      const normalizedPath = normalizeHeadingPath(table.rows, path.path);
      if (normalizedPath === null) {
        return table;
      }
      const newRows = setByPathInHeadings(
        table.rows,
        normalizedPath,
        newValue as Partial<Row>,
      );
      return { ...table, rows: newRows };
    }
    case "ColumnHeading": {
      const normalizedPath = normalizeHeadingPath(table.columns, path.path);
      if (normalizedPath === null) {
        return table;
      }
      const newColumns = setByPathInHeadings(
        table.columns,
        normalizedPath,
        newValue as Partial<Column>,
      );
      return { ...table, columns: newColumns };
    }
    case "Cell": {
      const normalizedRowPath = normalizeHeadingPath(table.rows, path.rowPath);
      if (normalizedRowPath === null) {
        return table;
      }
      const colIndex = normalizeToIndex(table.columns, path.colPath);
      if (colIndex === null) {
        return table;
      }
      const newRows = setCellInRow(
        table.rows,
        normalizedRowPath,
        colIndex,
        newValue as Partial<Cell>,
      );
      return { ...table, rows: newRows };
    }
  }
};

export const serializePath = (table: Body, path: TablePath): string => {
  switch (path.type) {
    case "TopLeft":
      return JSON.stringify({ type: "TopLeft" });
    case "RowHeading": {
      const normalized = normalizeHeadingPath(table.rows, path.path);
      return JSON.stringify({
        type: "RowHeading",
        path: normalized ?? path.path,
      });
    }
    case "ColumnHeading": {
      const normalized = normalizeHeadingPath(table.columns, path.path);
      return JSON.stringify({
        type: "ColumnHeading",
        path: normalized ?? path.path,
      });
    }
    case "Cell": {
      const rowIndex =
        typeof path.rowPath === "number"
          ? path.rowPath
          : headingPathToIndex(table.rows, path.rowPath);
      const colIndex =
        typeof path.colPath === "number"
          ? path.colPath
          : headingPathToIndex(table.columns, path.colPath);
      return JSON.stringify({
        type: "Cell",
        rowPath: rowIndex ?? path.rowPath,
        colPath: colIndex ?? path.colPath,
      });
    }
  }
};

export type Movement =
  | "Up"
  | "Down"
  | "Left"
  | "Right"
  | "Tab"
  | "ShiftTab"
  | "Home"
  | "End";

export const getMovement = (event: React.KeyboardEvent): Movement | null => {
  switch (event.key) {
    case "ArrowUp":
      return "Up";
    case "ArrowDown":
      return "Down";
    case "ArrowLeft":
      return "Left";
    case "ArrowRight":
      return "Right";
    case "Tab":
      return event.shiftKey ? "ShiftTab" : "Tab";
    case "Home":
      return "Home";
    case "End":
      return "End";
    default:
      return null;
  }
};

const headingParent = (
  headings: Heading[],
  target: HeadingPath,
): HeadingPath | null => {
  const normalized = normalizeHeadingPath(headings, target);
  if (
    normalized === null ||
    normalized.length === 0 ||
    normalized.length === 1
  ) {
    return null;
  }
  return normalized.slice(0, -1);
};

const headingFirstChild = (
  headings: Heading[],
  target: HeadingPath,
): HeadingPath | null => {
  const normalized = normalizeHeadingPath(headings, target);
  if (normalized === null) {
    return null;
  }
  const heading = followHeadingPath(headings, normalized);
  if (heading === null) {
    return null;
  }
  const children = headingChildren(heading);
  if (children.length === 0) {
    return null;
  }
  return [...normalized, 0];
};

const adjacentInList = (
  paths: HeadingPath[],
  target: HeadingPath,
  direction: "Next" | "Previous",
): HeadingPath | null => {
  const targetStr = target.join(",");
  const index = paths.findIndex((p) => p.join(",") === targetStr);
  if (index === -1) {
    return null;
  }
  const adjacentIndex = direction === "Next" ? index + 1 : index - 1;
  return paths[adjacentIndex] ?? null;
};

const collectPathsAtDepth = (
  headings: Heading[],
  depth: number,
  prefix: HeadingPath = [],
): HeadingPath[] => {
  const paths: HeadingPath[] = [];
  for (let i = 0; i < headings.length; i++) {
    const heading = headings[i]!;
    const children = headingChildren(heading);
    const path = [...prefix, i];
    if (depth === 1 || children.length === 0) {
      paths.push(path);
    } else {
      paths.push(...collectPathsAtDepth(children, depth - 1, path));
    }
  }
  return paths;
};

const lastCellOfRow = (row: Row): number | null => {
  if (row.type === "Group") {
    if (row.rows.length === 0) {
      return null;
    }
    return lastCellOfRow(row.rows[0]!);
  } else {
    if (row.cells.length === 0) {
      return null;
    }
    return row.cells.length - 1;
  }
};

const collectPathsDFS = (
  headings: Heading[],
  prefix: HeadingPath = [],
): HeadingPath[] => {
  const paths: HeadingPath[] = [];
  for (let i = 0; i < headings.length; i++) {
    const path = [...prefix, i];
    paths.push(path);
    paths.push(...collectPathsDFS(headingChildren(headings[i]!), path));
  }
  return paths;
};

const lastLeafColumn = (headings: Heading[]): HeadingPath | null => {
  const inner = (
    headings: Heading,
    currentPath: HeadingPath,
  ): { path: HeadingPath; heading: Heading } | null => {
    const children = headingChildren(headings);
    if (children.length === 0) {
      return { path: currentPath, heading: headings };
    }
    const lastChild = children[children.length - 1]!;
    return inner(lastChild, [...currentPath, children.length - 1]);
  };
  return (
    inner({ type: "Group", heading: "", columns: headings } as Column, [])
      ?.path ?? null
  );
};

const isLastLeafColumn = (table: Body, path: HeadingPath): boolean => {
  const last = lastLeafColumn(table.columns);
  if (last === null) {
    return false;
  }
  const lastAsTablePath: ColumnHeadingPath = {
    type: "ColumnHeading",
    path: last,
  };
  const pathAsTablePath: ColumnHeadingPath = { type: "ColumnHeading", path };
  return isPathEqual(table, lastAsTablePath, pathAsTablePath);
};

/**
 * Gets the next or previous sibling of the heading at the given path, where two headings that
 * are adjacent in the table (i.e. they are at the same depth in the headings tree and they either have the
 * same parent or their parents are siblings) are considered siblings.
 * @param headings Either `table.rows` or `table.columns`.
 * @param target
 * @param direction
 */
const headingSiblingByDepth = (
  headings: Heading[],
  target: HeadingPath,
  direction: "Next" | "Previous",
): HeadingPath | null => {
  return adjacentInList(
    collectPathsAtDepth(headings, target.length),
    target,
    direction,
  );
};

/**
 * Gets the next or previous sibling of the heading at the given path, where two headings that are adjacent
 * in a depth-first traversal of the headings tree are considered siblings.
 * @param headings Either `table.rows` or `table.columns`.
 * @param target
 * @param direction
 */
const headingSiblingByTree = (
  headings: Heading[],
  target: HeadingPath,
  direction: "Next" | "Previous",
): HeadingPath | null => {
  return adjacentInList(collectPathsDFS(headings), target, direction);
};

/**
 * Moves the given path in the given direction, returning the new path, or the same path if the movement
 * is not possible. If the movement would exit the table (and should thus be handled by the browser's default
 * behavior for tab/shift-tab), this function will return null.
 * "Tab" and "ShiftTab" are different from "Left" and "Right" when it comes to headings in particular;
 * "Left" and "Right" can traverse between sub-headings at the same depth, whereas "Tab" and "ShiftTab"
 * will go to the next heading in a depth-first traversal of the headings tree, regardless of depth.
 * Additionally, "Tab" and "ShiftTab" will move from the last cell in a row to the first cell in the next row,
 * whereas "Right" and "Left" will not "wrap around".
 * @param table
 * @param path
 * @param movement
 */
export const move = (
  table: Body,
  path: TablePath,
  movement: Movement,
): TablePath | null => {
  const currentElement = getByPath(table, path);
  if (currentElement === null) {
    return path;
  }

  switch (elementKind(currentElement)) {
    case "TopLeft": {
      switch (movement) {
        case "Right":
        case "Tab":
          if (table.columns.length > 0) {
            return { type: "ColumnHeading", path: 0 };
          }
          break;
        case "Down":
          if (table.rows.length > 0) {
            return { type: "RowHeading", path: 0 };
          }
          break;
        case "End":
          if (table.columns.length > 0) {
            return { type: "ColumnHeading", path: table.columns.length - 1 };
          }
          break;
        case "ShiftTab":
          return null;
        case "Left":
        case "Up":
        case "Home":
          return path;
      }
      break;
    }
    case "RowHeading": {
      const row = currentElement as Row;
      const isLeaf = row.type === "Individual";
      const rowPath = normalizeHeadingPath(
        table.rows,
        (path as RowHeadingPath).path,
      );
      if (rowPath === null) {
        return path;
      }
      const parentPath = headingParent(table.rows, rowPath);

      // if this is the first row, then Up should move to the top-left cell
      if (rowPath.every((index) => index === 0) && movement === "Up") {
        return { type: "TopLeft" };
      }

      // fast path for moving to the parent
      if (movement === "Left") {
        if (parentPath !== null) {
          return { type: "RowHeading", path: parentPath };
        } else {
          return path;
        }
      }

      if (movement === "Home") {
        if (rowPath.length < 2) {
          return { type: "TopLeft" };
        } else {
          return { type: "RowHeading", path: [rowPath[0]!] };
        }
      } else if (movement === "End") {
        const lastCellIndex = lastCellOfRow(row);
        if (lastCellIndex !== null) {
          return { type: "Cell", rowPath: rowPath, colPath: lastCellIndex };
        } else {
          return path;
        }
      }

      // fast path for moving to the first cell in this row
      if (isLeaf && (movement === "Right" || movement === "Tab")) {
        if (row.cells.length > 0) {
          return {
            type: "Cell",
            rowPath: (path as RowHeadingPath).path,
            colPath: 0,
          };
        }
      }

      // first child of this row group
      if (!isLeaf && movement === "Right") {
        const firstChildPath = headingFirstChild(table.rows, rowPath);
        if (firstChildPath !== null) {
          return { type: "RowHeading", path: firstChildPath };
        }
      }

      // move to last column header
      if (movement === "ShiftTab" && rowPath.every((index) => index === 0)) {
        if (table.columns.length > 0) {
          const lastColPath = lastLeafColumn(table.columns);
          if (lastColPath !== null) {
            return { type: "ColumnHeading", path: lastColPath };
          } else {
            return { type: "TopLeft" };
          }
        }
      }

      // all special cases handled; now handle siblings
      if (movement === "Tab") {
        const nextSiblingPath = headingSiblingByTree(
          table.rows,
          rowPath,
          "Next",
        );
        if (nextSiblingPath !== null) {
          return { type: "RowHeading", path: nextSiblingPath };
        }
        return path;
      } else if (movement === "ShiftTab") {
        const prevSiblingPath = headingSiblingByTree(
          table.rows,
          rowPath,
          "Previous",
        );
        if (prevSiblingPath !== null) {
          return { type: "RowHeading", path: prevSiblingPath };
        }
        return path;
      } else if (movement === "Down") {
        const nextSiblingPath = headingSiblingByDepth(
          table.rows,
          rowPath,
          "Next",
        );
        if (nextSiblingPath !== null) {
          return { type: "RowHeading", path: nextSiblingPath };
        }
        return path;
      } else if (movement === "Up") {
        const prevSiblingPath = headingSiblingByDepth(
          table.rows,
          rowPath,
          "Previous",
        );
        if (prevSiblingPath !== null) {
          return { type: "RowHeading", path: prevSiblingPath };
        }
        return path;
      }

      // left and right already handled above

      break;
    }
    case "ColumnHeading": {
      const column = currentElement as Column;
      const isLeaf = column.type === "Individual";
      const colPath = normalizeHeadingPath(
        table.columns,
        (path as ColumnHeadingPath).path,
      );
      if (colPath === null) {
        return path;
      }

      // if this is the first column, then Left should move to the top-left cell
      if (
        colPath.every((index) => index === 0) &&
        (movement === "Left" || movement === "ShiftTab")
      ) {
        return { type: "TopLeft" };
      }

      // if this is the last column, then Tab should move to the first row header
      if (isLastLeafColumn(table, colPath) && movement === "Tab") {
        if (table.rows.length > 0) {
          return { type: "RowHeading", path: 0 };
        } else {
          return null;
        }
      }

      // fast path for moving to the parent
      if (movement === "Up") {
        const parentPath = headingParent(table.columns, colPath);
        if (parentPath !== null) {
          return { type: "ColumnHeading", path: parentPath };
        }
      }

      // fast path for moving to the first cell in this column
      if (isLeaf && movement === "Down") {
        if (table.rows.length > 0) {
          return {
            type: "Cell",
            rowPath: 0,
            colPath: (path as ColumnHeadingPath).path,
          };
        }
      }

      // first child of this column group
      if (!isLeaf && movement === "Down") {
        const firstChildPath = headingFirstChild(table.columns, colPath);
        if (firstChildPath !== null) {
          return { type: "ColumnHeading", path: firstChildPath };
        }
      }

      // all special cases handled; now handle siblings
      if (movement === "Tab") {
        const nextSiblingPath = headingSiblingByTree(
          table.columns,
          colPath,
          "Next",
        );
        if (nextSiblingPath !== null) {
          return { type: "ColumnHeading", path: nextSiblingPath };
        }
        return path;
      } else if (movement === "ShiftTab") {
        const prevSiblingPath = headingSiblingByTree(
          table.columns,
          colPath,
          "Previous",
        );
        if (prevSiblingPath !== null) {
          return { type: "ColumnHeading", path: prevSiblingPath };
        }
        return path;
      } else if (movement === "Right") {
        const nextSiblingPath = headingSiblingByDepth(
          table.columns,
          colPath,
          "Next",
        );
        if (nextSiblingPath !== null) {
          return { type: "ColumnHeading", path: nextSiblingPath };
        }
        return path;
      } else if (movement === "Left") {
        const prevSiblingPath = headingSiblingByDepth(
          table.columns,
          colPath,
          "Previous",
        );
        if (prevSiblingPath !== null) {
          return { type: "ColumnHeading", path: prevSiblingPath };
        }
        return path;
      }

      // up and down already handled above

      break;
    }
    case "Cell": {
      const rowIndex = normalizeToIndex(table.rows, (path as CellPath).rowPath);
      const colIndex = normalizeToIndex(
        table.columns,
        (path as CellPath).colPath,
      );

      const isFirstRow = rowIndex === 0;
      const isFirstCol = colIndex === 0;
      const isLastRow = rowIndex === numLeaves(table.rows) - 1;
      const isLastCol = colIndex === numLeaves(table.columns) - 1;

      if (rowIndex === null || colIndex === null) {
        return path;
      }

      switch (movement) {
        case "Up":
          if (!isFirstRow) {
            return { type: "Cell", rowPath: rowIndex - 1, colPath: colIndex };
          } else {
            return { type: "ColumnHeading", path: (path as CellPath).colPath };
          }
        case "Down":
          if (!isLastRow) {
            return { type: "Cell", rowPath: rowIndex + 1, colPath: colIndex };
          }
          break;
        case "Left":
        case "ShiftTab":
          if (!isFirstCol) {
            return { type: "Cell", rowPath: rowIndex, colPath: colIndex - 1 };
          } else {
            return { type: "RowHeading", path: (path as CellPath).rowPath };
          }
        case "Right":
          if (!isLastCol) {
            return { type: "Cell", rowPath: rowIndex, colPath: colIndex + 1 };
          }
          break;
        case "Tab":
          if (isLastCol && isLastRow) {
            return null;
          }
          if (!isLastCol) {
            return { type: "Cell", rowPath: rowIndex, colPath: colIndex + 1 };
          } else {
            // move to the next heading
            // the next heading is defined by the next heading in the depth-first traversal
            // of the tree
            const nextRowHeadingPath = headingSiblingByTree(
              table.rows,
              normalizeHeadingPath(table.rows, (path as CellPath).rowPath)!,
              "Next",
            );
            if (nextRowHeadingPath !== null) {
              return { type: "RowHeading", path: nextRowHeadingPath };
            } else {
              return null;
            }
          }
        case "Home":
          return { type: "RowHeading", path: (path as CellPath).rowPath };
        case "End":
          const rowHeading = followHeadingPath(
            table.rows,
            normalizeHeadingPath(table.rows, (path as CellPath).rowPath)!,
          );
          if (rowHeading === null) {
            return path;
          }
          const lastCellIndex = lastCellOfRow(rowHeading);
          if (lastCellIndex !== null) {
            return {
              type: "Cell",
              rowPath: (path as CellPath).rowPath,
              colPath: lastCellIndex,
            };
          } else {
            return path;
          }
      }
    }
  }

  return path;
};
