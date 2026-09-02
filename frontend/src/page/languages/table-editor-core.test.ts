import { expect, test } from "bun:test";
import {
  apply,
  flatRows,
  initialState,
  moveFocus,
  type Body,
  type GridCell,
} from "./table-editor-core";

const body: Body<GridCell> = {
  columns: [
    {
      type: "Group",
      heading: "Number",
      columns: [
        { type: "Individual", heading: "Singular" },
        { type: "Individual", heading: "Plural" },
      ],
    },
  ],
  rows: [
    {
      type: "Group",
      heading: "Case",
      rows: [
        {
          type: "Individual",
          heading: "Nominative",
          cells: [{}, {}],
        },
        {
          type: "Individual",
          heading: "Accusative",
          cells: [{}, {}],
        },
      ],
    },
  ],
};

test("moves between the top-left corner and grouped headings", () => {
  expect(moveFocus(body, { type: "TopLeft" }, "Right")).toEqual({
    type: "ColumnHeading",
    path: [0],
  });
  expect(moveFocus(body, { type: "ColumnHeading", path: [0] }, "Down")).toEqual(
    { type: "ColumnHeading", path: [0, 0] },
  );
  expect(moveFocus(body, { type: "RowHeading", path: [0] }, "Right")).toEqual({
    type: "RowHeading",
    path: [0, 0],
  });
  expect(moveFocus(body, { type: "RowHeading", path: [0, 0] }, "Up")).toEqual({
    type: "TopLeft",
  });
});

test("moves between leaf headings and cells", () => {
  expect(
    moveFocus(body, { type: "ColumnHeading", path: [0, 1] }, "Down"),
  ).toEqual({ type: "Cell", row: 0, column: 1 });
  expect(moveFocus(body, { type: "Cell", row: 0, column: 0 }, "Left")).toEqual({
    type: "RowHeading",
    path: [0, 0],
  });
  expect(moveFocus(body, { type: "Cell", row: 0, column: 0 }, "Up")).toEqual({
    type: "ColumnHeading",
    path: [0, 0],
  });
});

test("tab traverses headings and then enters rows", () => {
  expect(moveFocus(body, { type: "ColumnHeading", path: [0] }, "Tab")).toEqual({
    type: "ColumnHeading",
    path: [0, 0],
  });
  expect(
    moveFocus(body, { type: "ColumnHeading", path: [0, 1] }, "Tab"),
  ).toEqual({ type: "RowHeading", path: [0] });
  expect(moveFocus(body, { type: "RowHeading", path: [0, 0] }, "Tab")).toEqual({
    type: "Cell",
    row: 0,
    column: 0,
  });
});

test("movement skips cells covered by a merge", () => {
  const merged: Body<GridCell> = {
    columns: body.columns,
    rows: [
      {
        type: "Individual",
        heading: "A",
        cells: [{ colspan: 2 }, {}],
      },
      {
        type: "Individual",
        heading: "B",
        cells: [{}, {}],
      },
    ],
  };
  expect(
    moveFocus(merged, { type: "Cell", row: 0, column: 0 }, "Right"),
  ).toEqual({ type: "Cell", row: 0, column: 0 });
  expect(moveFocus(merged, { type: "Cell", row: 1, column: 1 }, "Up")).toEqual({
    type: "Cell",
    row: 0,
    column: 0,
  });
});

test("merging expands to include an intersected merged region", () => {
  const initial: Body<GridCell> = {
    columns: [
      { type: "Individual", heading: "A" },
      { type: "Individual", heading: "B" },
      { type: "Individual", heading: "C" },
    ],
    rows: [
      {
        type: "Individual",
        heading: "A",
        cells: [{}, { colspan: 2 }, {}],
      },
      {
        type: "Individual",
        heading: "B",
        cells: [{}, {}, {}],
      },
    ],
  };
  const state = apply<GridCell>(
    initialState<GridCell>(initial),
    {
      type: "Merge",
      first: { row: 0, column: 0 },
      second: { row: 1, column: 1 },
    },
    { createCell: () => ({}), mergeCells: (anchor) => anchor },
  );
  const anchor = flatRows(state.body.rows)[0]!.row.cells[0]!;
  expect(anchor.rowspan).toBe(2);
  expect(anchor.colspan).toBe(3);
});

test("deleting a row group shrinks a merge by every removed leaf", () => {
  const initial: Body<GridCell> = {
    columns: [{ type: "Individual", heading: "A" }],
    rows: [
      {
        type: "Individual",
        heading: "Before",
        cells: [{ rowspan: 3 }],
      },
      {
        type: "Group",
        heading: "Group",
        rows: [
          { type: "Individual", heading: "One", cells: [{}] },
          { type: "Individual", heading: "Two", cells: [{}] },
        ],
      },
    ],
  };
  const state = apply<GridCell>(
    initialState<GridCell>(initial),
    { type: "DeleteHeading", kind: "row", path: [1] },
    { createCell: () => ({}), mergeCells: (anchor) => anchor },
  );
  expect(flatRows(state.body.rows)[0]!.row.cells[0]!.rowspan).toBe(1);
});
