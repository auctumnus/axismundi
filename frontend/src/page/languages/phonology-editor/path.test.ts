import { expect, test } from "bun:test";
import {
  countLeaves,
  getByPath,
  headingPathToIndex,
  indexToHeadingPath,
  move,
  setByPath,
  type TablePath,
} from "./path";
import { TOP_LEFT_CELL, type Body, type Cell, type Row } from "./table";

const cell = (...phonemes: string[]): Cell => ({
  phonemes: phonemes.map((p) => ({ text: p, annotations: [] })),
});

test("countLeaves base case", () => {
  const individualRow: Row = {
    type: "Individual",
    heading: "Row 1",
    cells: [],
  };

  expect(countLeaves(individualRow)).toBe(1);
});

test("countLeaves group with individuals", () => {
  const groupRow: Row = {
    type: "Group",
    heading: "Group 1",
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      { type: "Individual", heading: "Row 2", cells: [] },
    ],
  };

  expect(countLeaves(groupRow)).toBe(2);
});

test("countLeaves nested groups", () => {
  const nestedGroupRow: Row = {
    type: "Group",
    heading: "Group 1",
    rows: [
      {
        type: "Group",
        heading: "Subgroup 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
  };

  expect(countLeaves(nestedGroupRow)).toBe(3);
});

test("headingPathToIndex simple case", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      { type: "Individual", heading: "Row 2", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  expect(headingPathToIndex(table.rows, [0])).toBe(0);
  expect(headingPathToIndex(table.rows, [1])).toBe(1);
});

test("headingPathToIndex nested groups", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  expect(headingPathToIndex(table.rows, [0, 0])).toBe(0);
  expect(headingPathToIndex(table.rows, [0, 1])).toBe(1);
  expect(headingPathToIndex(table.rows, [1])).toBe(2);
});

test("headingPathToIndex invalid path", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };

  expect(headingPathToIndex(table.rows, [1])).toBeNull();
  expect(headingPathToIndex(table.rows, [0, 0])).toBeNull();
});

test("indexToHeadingPath simple case", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      { type: "Individual", heading: "Row 2", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  expect(indexToHeadingPath(table.rows, 0)).toEqual([0]);
  expect(indexToHeadingPath(table.rows, 1)).toEqual([1]);
});

test("indexToHeadingPath nested groups", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  expect(indexToHeadingPath(table.rows, 0)).toEqual([0, 0]);
  expect(indexToHeadingPath(table.rows, 1)).toEqual([0, 1]);
  expect(indexToHeadingPath(table.rows, 2)).toEqual([1]);
});

test("indexToHeadingPath invalid index", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };

  expect(indexToHeadingPath(table.rows, 1)).toBeNull();
});

test("getByPath with RowHeading and HeadingPath", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0, 1] };
  const result = getByPath(table, path);
  // @ts-ignore
  expect(result).toEqual(table.rows[0]!.rows[1]);
});

test("getByPath with RowHeading and index path", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: 1 };
  const result = getByPath(table, path);
  // @ts-ignore
  expect(result).toEqual(table.rows[0]!.rows[1]);
});

test("getByPath with RowHeading and invalid index path", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: 5 };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with RowHeading and invalid HeadingPath", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0, 5] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with RowHeading and invalid HeadingPath trying to access individual as group", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [1, 0, 0] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with empty HeadingPath", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with out-of-bounds HeadingPath", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [2] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with ColumnHeading and HeadingPath", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Column 1" },
          { type: "Individual", heading: "Column 2" },
        ],
      },
      { type: "Individual", heading: "Column 3" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "ColumnHeading", path: [0, 1] };
  const result = getByPath(table, path);
  // @ts-ignore
  expect(result).toEqual(table.columns[0]!.columns[1]);
});

test("getByPath with ColumnHeading and index path", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Column 1" },
          { type: "Individual", heading: "Column 2" },
        ],
      },
      { type: "Individual", heading: "Column 3" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "ColumnHeading", path: 1 };
  const result = getByPath(table, path);
  // @ts-ignore
  expect(result).toEqual(table.columns[0]!.columns[1]);
});

test("getByPath with TopLeft path", () => {
  const table: Body = {
    rows: [],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "TopLeft" };
  const result = getByPath(table, path);
  expect(result).toEqual(TOP_LEFT_CELL);
});

test("getByPath with cell path", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a", "b"), cell("c")],
      },
      { type: "Individual", heading: "Row 2", cells: [cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: [1] };
  const result = getByPath(table, path);
  // @ts-ignore
  expect(result).toEqual(table.rows[0]!.cells[1]);
});

test("getByPath with invalid cell path (invalid row path)", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a", "b"), cell("c")],
      },
      { type: "Individual", heading: "Row 2", cells: [cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: 5, colPath: [0] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with invalid cell path (invalid column path)", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a", "b"), cell("c")],
      },
      { type: "Individual", heading: "Row 2", cells: [cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 5 };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with ColumnHeading and invalid index path", () => {
  const table: Body = {
    rows: [],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "ColumnHeading", path: 5 };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with cell path using invalid HeadingPath colPath", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Column 1" }],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: [5] };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("getByPath with cell path where row followHeadingPath returns null", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
      },
    ],
    columns: [{ type: "Individual", heading: "Column 1" }],
    annotations: [],
  };

  // path [0, 5] passes normalizeHeadingPath (it's already a HeadingPath)
  // but followHeadingPath returns null because child 5 doesn't exist
  const path: TablePath = { type: "Cell", rowPath: [0, 5], colPath: 0 };
  const result = getByPath(table, path);
  expect(result).toBeNull();
});

test("setByPath with cell path updates the correct cell", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: [1] };
  const result = setByPath(table, path, cell("x", "y"));
  const expected: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a"), cell("x", "y")],
      },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };
  expect(result).toEqual(expected);
});

test("setByPath on top left cell does not modify the table", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "TopLeft" };
  // @ts-ignore
  const result = setByPath(table, path, cell("x"));
  expect(result).toEqual(table);
});

test("setByPath on row heading updates the correct row", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const result = setByPath(table, path, {
    type: "Individual",
    heading: "New Row 1",
    cells: [cell("x")],
  });
  const expected: Body = {
    rows: [
      { type: "Individual", heading: "New Row 1", cells: [cell("x")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };
  expect(result).toEqual(expected);
});

test("setByPath on column heading updates the correct column", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "ColumnHeading", path: [0] };
  const result = setByPath(table, path, {
    type: "Individual",
    heading: "New Column 1",
  });
  const expected: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c")] },
    ],
    columns: [
      { type: "Individual", heading: "New Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };
  expect(result).toEqual(expected);
});

test("setByPath on row heading with groups updates the correct row", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [cell("a")] },
          { type: "Individual", heading: "Row 2", cells: [cell("b")] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [cell("c")] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  // @ts-ignore
  const result = setByPath(table, path, {
    type: "Group",
    heading: "New Group 1",
    rows: table.rows[0]!.rows,
  });
  const expected: Body = {
    rows: [
      {
        type: "Group",
        heading: "New Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [cell("a")] },
          { type: "Individual", heading: "Row 2", cells: [cell("b")] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [cell("c")] },
    ],
    columns: [],
    annotations: [],
  };
  expect(result).toEqual(expected);
});

test("movement from first row heading to top-left cell", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      { type: "Individual", heading: "Row 2", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const movement = "Up";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "TopLeft" });
});

test("movement from first column heading to top-left cell", () => {
  const table: Body = {
    rows: [],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "ColumnHeading", path: [0] };
  const movement = "Left";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "TopLeft" });
});

test("movement: tab from row group parent to first child", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const movement = "Tab";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0, 0] });
});

test("movement: shift+tab from row group child to parent", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0, 0] };
  const movement = "ShiftTab";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0] });
});

test("movement: down to sibling row by depth", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 2", cells: [] },
          { type: "Individual", heading: "Row 3", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 4", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const movement = "Down";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [1] });
});

test("movement: up to sibling row by depth", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 2", cells: [] },
          { type: "Individual", heading: "Row 3", cells: [] },
        ],
      },
      { type: "Individual", heading: "Row 4", cells: [] },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [1, 0] };
  const movement = "Up";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0] });
});

test("movement: up to sibling row by depth from group to group", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
      {
        type: "Group",
        heading: "Group 2",
        rows: [
          { type: "Individual", heading: "Row 3", cells: [] },
          { type: "Individual", heading: "Row 4", cells: [] },
        ],
      },
    ],
    columns: [],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [1, 0] };
  const movement = "Up";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0, 1] });
});

test("movement: right to first cell in row from row heading", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const movement = "Right";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "Cell", rowPath: [0], colPath: 0 });
});

test("movement: left to row heading from first cell in row", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 0 };
  const movement = "Left";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0] });
});

test("movement: right from first cell in row to second cell in row", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 0 };
  const movement = "Right";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "Cell", rowPath: 0, colPath: 1 });
});

test("movement: left from second cell in row to first cell in row", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 1 };
  const movement = "Left";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "Cell", rowPath: 0, colPath: 0 });
});

test("movement: down from first cell in row to first cell in next row", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 0 };
  const movement = "Down";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "Cell", rowPath: 1, colPath: 0 });
});

test("movement: tab from last cell in row to next row heading, next row heading is individual", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0], colPath: 1 };
  const movement = "Tab";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [1] });
});

test("movement: tab from last cell in row to next row heading, next row heading is sibling in group", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          {
            type: "Individual",
            heading: "Row 1",
            cells: [cell("a"), cell("b")],
          },
          {
            type: "Individual",
            heading: "Row 2",
            cells: [cell("c"), cell("d")],
          },
        ],
      },
      { type: "Individual", heading: "Row 3", cells: [cell("e"), cell("f")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [0, 0], colPath: 1 };
  const movement = "Tab";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "RowHeading", path: [0, 1] });
});

// ===== TopLeft movement tests =====

test("movement: right from top-left to first column heading", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Column 1" }],
    annotations: [],
  };
  expect(move(table, { type: "TopLeft" }, "Right")).toEqual({
    type: "ColumnHeading",
    path: 0,
  });
});

test("movement: tab from top-left to first column heading", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Column 1" }],
    annotations: [],
  };
  expect(move(table, { type: "TopLeft" }, "Tab")).toEqual({
    type: "ColumnHeading",
    path: 0,
  });
});

test("movement: down from top-left to first row heading", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  expect(move(table, { type: "TopLeft" }, "Down")).toEqual({
    type: "RowHeading",
    path: 0,
  });
});

test("movement: end from top-left to last column heading", () => {
  const table: Body = {
    rows: [],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
      { type: "Individual", heading: "Column 3" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "TopLeft" }, "End")).toEqual({
    type: "ColumnHeading",
    path: 2,
  });
});

test("movement: home from top-left returns same path", () => {
  const table: Body = { rows: [], columns: [], annotations: [] };
  const path: TablePath = { type: "TopLeft" };
  expect(move(table, path, "Home")).toEqual(path);
});

// ===== RowHeading movement tests (additional) =====

test("movement: left from row heading with no parent stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: [0] };
  expect(move(table, path, "Left")).toEqual(path);
});

test("movement: home from top-level row heading goes to top-left", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [] },
      { type: "Individual", heading: "Row 2", cells: [] },
    ],
    columns: [],
    annotations: [],
  };
  expect(move(table, { type: "RowHeading", path: [1] }, "Home")).toEqual({
    type: "TopLeft",
  });
});

test("movement: home from nested row heading goes to top-level parent", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          {
            type: "Group",
            heading: "Subgroup 1",
            rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
          },
        ],
      },
    ],
    columns: [],
    annotations: [],
  };
  expect(move(table, { type: "RowHeading", path: [0, 0, 0] }, "Home")).toEqual({
    type: "RowHeading",
    path: [0],
  });
});

test("movement: end from row heading goes to last cell", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a"), cell("b"), cell("c")],
      },
    ],
    columns: [
      { type: "Individual", heading: "C1" },
      { type: "Individual", heading: "C2" },
      { type: "Individual", heading: "C3" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "RowHeading", path: [0] }, "End")).toEqual({
    type: "Cell",
    rowPath: [0],
    colPath: 2,
  });
});

test("movement: end from row heading with no cells stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: [0] };
  expect(move(table, path, "End")).toEqual(path);
});

test("movement: right from group row heading goes to first child", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
    ],
    columns: [],
    annotations: [],
  };
  expect(move(table, { type: "RowHeading", path: [0] }, "Right")).toEqual({
    type: "RowHeading",
    path: [0, 0],
  });
});

test("movement: tab from last row heading stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: [0] };
  expect(move(table, path, "Tab")).toEqual(path);
});

test("movement: shifttab from first row heading stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: [0] };
  expect(move(table, path, "ShiftTab")).toEqual(path);
});

test("movement: down from last row heading stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: [0] };
  expect(move(table, path, "Down")).toEqual(path);
});

test("movement: up from row heading at boundary stays", () => {
  const table: Body = {
    rows: [
      {
        type: "Group",
        heading: "Group 1",
        rows: [
          { type: "Individual", heading: "Row 1", cells: [] },
          { type: "Individual", heading: "Row 2", cells: [] },
        ],
      },
    ],
    columns: [],
    annotations: [],
  };
  // [0, 0] is depth 2, up by depth looks for previous at depth 2 — there's none before it
  const path: TablePath = { type: "RowHeading", path: [0, 0] };
  expect(move(table, path, "Up")).toEqual({ type: "TopLeft" });
});

// ===== ColumnHeading movement tests =====

test("movement: up from column heading to parent", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Col 1" },
          { type: "Individual", heading: "Col 2" },
        ],
      },
    ],
    annotations: [],
  };
  expect(move(table, { type: "ColumnHeading", path: [0, 0] }, "Up")).toEqual({
    type: "ColumnHeading",
    path: [0],
  });
});

test("movement: down from leaf column heading to first cell in column", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  expect(move(table, { type: "ColumnHeading", path: [0] }, "Down")).toEqual({
    type: "Cell",
    rowPath: 0,
    colPath: [0],
  });
});

test("movement: down from group column heading to first child", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Col 1" },
          { type: "Individual", heading: "Col 2" },
        ],
      },
    ],
    annotations: [],
  };
  expect(move(table, { type: "ColumnHeading", path: [0] }, "Down")).toEqual({
    type: "ColumnHeading",
    path: [0, 0],
  });
});

test("movement: tab/shifttab through column headings", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Col 1" },
          { type: "Individual", heading: "Col 2" },
        ],
      },
      { type: "Individual", heading: "Col 3" },
    ],
    annotations: [],
  };
  // Tab from group to first child (tree traversal)
  expect(move(table, { type: "ColumnHeading", path: [0] }, "Tab")).toEqual({
    type: "ColumnHeading",
    path: [0, 0],
  });
  // Tab from first child to second child
  expect(move(table, { type: "ColumnHeading", path: [0, 0] }, "Tab")).toEqual({
    type: "ColumnHeading",
    path: [0, 1],
  });
  // ShiftTab back
  expect(
    move(table, { type: "ColumnHeading", path: [0, 1] }, "ShiftTab"),
  ).toEqual({ type: "ColumnHeading", path: [0, 0] });
});

test("movement: tab from last column heading stays", () => {
  const table: Body = {
    rows: [],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  const path: TablePath = { type: "ColumnHeading", path: [0] };
  expect(move(table, path, "Tab")).toEqual(path);
});

test("movement: shifttab from first column heading stays", () => {
  const table: Body = {
    rows: [],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  const path: TablePath = { type: "ColumnHeading", path: [0] };
  expect(move(table, path, "ShiftTab")).toEqual(path);
});

test("movement: right/left through column headings by depth", () => {
  const table: Body = {
    rows: [],
    columns: [
      { type: "Individual", heading: "Col 1" },
      { type: "Individual", heading: "Col 2" },
      { type: "Individual", heading: "Col 3" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "ColumnHeading", path: [0] }, "Right")).toEqual({
    type: "ColumnHeading",
    path: [1],
  });
  expect(move(table, { type: "ColumnHeading", path: [1] }, "Left")).toEqual({
    type: "ColumnHeading",
    path: [0],
  });
});

test("movement: right from last column heading stays", () => {
  const table: Body = {
    rows: [],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  const path: TablePath = { type: "ColumnHeading", path: [0] };
  expect(move(table, path, "Right")).toEqual(path);
});

test("movement: left from first column heading (all zeros) goes to top-left", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Col 1" },
          { type: "Individual", heading: "Col 2" },
        ],
      },
    ],
    annotations: [],
  };
  // [0, 0] has every index === 0, so Left goes to TopLeft
  expect(move(table, { type: "ColumnHeading", path: [0, 0] }, "Left")).toEqual({
    type: "TopLeft",
  });
});

test("movement: left from non-first column heading by depth", () => {
  const table: Body = {
    rows: [],
    columns: [
      {
        type: "Group",
        heading: "Group 1",
        columns: [
          { type: "Individual", heading: "Col 1" },
          { type: "Individual", heading: "Col 2" },
        ],
      },
    ],
    annotations: [],
  };
  expect(move(table, { type: "ColumnHeading", path: [0, 1] }, "Left")).toEqual({
    type: "ColumnHeading",
    path: [0, 0],
  });
});

// ===== Cell movement tests (additional) =====

test("movement: up from first row cell to column heading", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Col 1" },
      { type: "Individual", heading: "Col 2" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "Cell", rowPath: 0, colPath: 1 }, "Up")).toEqual({
    type: "ColumnHeading",
    path: 1,
  });
});

test("movement: down from last row cell stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  const path: TablePath = { type: "Cell", rowPath: 0, colPath: 0 };
  expect(move(table, path, "Down")).toEqual(path);
});

test("movement: right from last col cell stays", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  const path: TablePath = { type: "Cell", rowPath: 0, colPath: 0 };
  expect(move(table, path, "Right")).toEqual(path);
});

test("movement: tab from non-last col cell moves right", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Col 1" },
      { type: "Individual", heading: "Col 2" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "Cell", rowPath: 0, colPath: 0 }, "Tab")).toEqual({
    type: "Cell",
    rowPath: 0,
    colPath: 1,
  });
});

test("movement: shifttab from first col cell goes to row heading", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Col 1" },
      { type: "Individual", heading: "Col 2" },
    ],
    annotations: [],
  };
  expect(
    move(table, { type: "Cell", rowPath: 0, colPath: 0 }, "ShiftTab"),
  ).toEqual({ type: "RowHeading", path: 0 });
});

test("movement: tab from last col wraps to next row heading", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a")] },
      { type: "Individual", heading: "Row 2", cells: [cell("b")] },
    ],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  expect(move(table, { type: "Cell", rowPath: 0, colPath: 0 }, "Tab")).toEqual({
    type: "RowHeading",
    path: [1],
  });
});

test("movement: tab from last col of last row returns null (no next row)", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [cell("a")] }],
    columns: [{ type: "Individual", heading: "Col 1" }],
    annotations: [],
  };
  // This is last row AND last col but only 1 row, so headingSiblingByTree returns null
  expect(
    move(table, { type: "Cell", rowPath: 0, colPath: 0 }, "Tab"),
  ).toBeNull();
});

test("movement: home from cell goes to row heading", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
    ],
    columns: [
      { type: "Individual", heading: "Col 1" },
      { type: "Individual", heading: "Col 2" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "Cell", rowPath: 0, colPath: 1 }, "Home")).toEqual(
    { type: "RowHeading", path: 0 },
  );
});

test("movement: end from cell goes to last cell in row", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a"), cell("b"), cell("c")],
      },
    ],
    columns: [
      { type: "Individual", heading: "C1" },
      { type: "Individual", heading: "C2" },
      { type: "Individual", heading: "C3" },
    ],
    annotations: [],
  };
  expect(move(table, { type: "Cell", rowPath: 0, colPath: 0 }, "End")).toEqual({
    type: "Cell",
    rowPath: 0,
    colPath: 2,
  });
});

test("movement: move with invalid path returns same path", () => {
  const table: Body = {
    rows: [{ type: "Individual", heading: "Row 1", cells: [] }],
    columns: [],
    annotations: [],
  };
  const path: TablePath = { type: "RowHeading", path: 99 };
  expect(move(table, path, "Up")).toEqual(path);
});

test("movement: tab from last row + last col to out of table (should be null)", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "Cell", rowPath: [1], colPath: [1] };
  const movement = "Tab";
  const result = move(table, path, movement);
  expect(result).toBeNull();
});

test("shift tab from top left to out of table (should be null)", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "TopLeft" };
  const movement = "ShiftTab";
  const result = move(table, path, movement);
  expect(result).toBeNull();
});

test("movement: up from top-left returns same path", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "TopLeft" };
  const movement = "Up";
  const result = move(table, path, movement);
  expect(result).toEqual(path);
});

test("movement: left from top-left returns same path (no parent)", () => {
  const table: Body = {
    rows: [
      { type: "Individual", heading: "Row 1", cells: [cell("a"), cell("b")] },
      { type: "Individual", heading: "Row 2", cells: [cell("c"), cell("d")] },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "TopLeft" };
  const movement = "Left";
  const result = move(table, path, movement);
  expect(result).toEqual(path);
});

test("movement: end to last cell in row", () => {
  const table: Body = {
    rows: [
      {
        type: "Individual",
        heading: "Row 1",
        cells: [cell("a"), cell("b"), cell("c")],
      },
    ],
    columns: [
      { type: "Individual", heading: "Column 1" },
      { type: "Individual", heading: "Column 2" },
      { type: "Individual", heading: "Column 3" },
    ],
    annotations: [],
  };

  const path: TablePath = { type: "RowHeading", path: [0] };
  const movement = "End";
  const result = move(table, path, movement);
  expect(result).toEqual({ type: "Cell", rowPath: [0], colPath: 2 });
});
