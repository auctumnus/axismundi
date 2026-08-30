import { expect, test } from "bun:test";
import {
  adjustSpansForColumnInsert,
  adjustSpansForColumnRemove,
  adjustSpansForRowInsert,
  adjustSpansForRowRemove,
  anchorOf,
  cellColspan,
  cellRowspan,
  coveredCells,
  isCovered,
  leafCellRows,
  mergeRect,
  withSpan,
} from "./spans";
import type { Cell, Row } from "./table";

const cell = (...phonemes: string[]): Cell => ({
  phonemes: phonemes.map((p) => ({ text: p, annotations: [] })),
});

const spanned = (rowspan: number, colspan: number, ...phonemes: string[]): Cell =>
  withSpan(cell(...phonemes), rowspan, colspan);

const row = (heading: string, ...cells: Cell[]): Row => ({
  type: "Individual",
  heading,
  cells,
});

const group = (heading: string, ...rows: Row[]): Row => ({
  type: "Group",
  heading,
  rows,
});

test("withSpan omits span properties when 1", () => {
  const c = withSpan(spanned(2, 3, "a"), 1, 1);
  expect(c).toEqual({ phonemes: [{ text: "a", annotations: [] }] });
  expect(cellRowspan(c)).toBe(1);
  expect(cellColspan(c)).toBe(1);
});

test("leafCellRows flattens groups in visual order", () => {
  const rows = [
    row("a", cell("1")),
    group("g", row("b", cell("2")), row("c", cell("3"))),
    row("d", cell("4")),
  ];
  expect(leafCellRows(rows).map((cells) => cells[0]!.phonemes[0]!.text)).toEqual(
    ["1", "2", "3", "4"],
  );
});

test("coveredCells maps hidden positions to their anchor", () => {
  // 2x3 grid, anchor at (0,0) spanning 2x2
  const rows = [
    row("r1", spanned(2, 2, "a"), cell(), cell("k")),
    row("r2", cell(), cell(), cell("g")),
  ];
  const covered = coveredCells(rows);
  expect(covered.size).toBe(3);
  expect(isCovered(covered, 0, 1)).toBe(true);
  expect(isCovered(covered, 1, 0)).toBe(true);
  expect(isCovered(covered, 1, 1)).toBe(true);
  expect(isCovered(covered, 0, 0)).toBe(false);
  expect(isCovered(covered, 0, 2)).toBe(false);
  expect(anchorOf(covered, 1, 1)).toEqual([0, 0]);
  expect(anchorOf(covered, 1, 2)).toEqual([1, 2]);
});

test("mergeRect expands to include partially overlapped merges", () => {
  // (0,1) has a 2x1 merge; merging (0,0) with (0,1) must pull in (1,1)
  const rows = [
    row("r1", cell("a"), spanned(2, 1, "b"), cell()),
    row("r2", cell(), cell(), cell("c")),
  ];
  const rect = mergeRect(rows, [0, 0], [0, 1]);
  expect(rect).toEqual({ top: 0, left: 0, bottom: 1, right: 1 });
});

test("mergeRect on plain cells is just the bounding box", () => {
  const rows = [
    row("r1", cell("a"), cell(), cell()),
    row("r2", cell(), cell(), cell("c")),
  ];
  expect(mergeRect(rows, [1, 2], [0, 0])).toEqual({
    top: 0,
    left: 0,
    bottom: 1,
    right: 2,
  });
});

test("column insert inside a merge widens it", () => {
  const rows = [row("r1", spanned(1, 2, "a"), cell(), cell("b"))];
  const adjusted = adjustSpansForColumnInsert(rows, 1);
  const cells = leafCellRows(adjusted)[0]!;
  expect(cellColspan(cells[0]!)).toBe(3);
});

test("column insert at a merge edge does not widen it", () => {
  const rows = [row("r1", spanned(1, 2, "a"), cell(), cell("b"))];
  // inserting exactly at the left edge (0) or just past the right edge (2)
  for (const at of [0, 2]) {
    const cells = leafCellRows(adjustSpansForColumnInsert(rows, at))[0]!;
    expect(cellColspan(cells[0]!)).toBe(2);
  }
});

test("column remove overlapping a merge shrinks it", () => {
  const rows = [row("r1", spanned(1, 3, "a"), cell(), cell(), cell("b"))];
  const cells = leafCellRows(adjustSpansForColumnRemove(rows, 1, 2))[0]!;
  expect(cellColspan(cells[0]!)).toBe(1);
  expect(cells[0]!.colspan).toBeUndefined();
});

test("row insert inside a merge stretches it", () => {
  const rows = [
    row("r1", spanned(2, 1, "a"), cell("b")),
    row("r2", cell(), cell("c")),
  ];
  const adjusted = adjustSpansForRowInsert(rows, 1);
  expect(cellRowspan(leafCellRows(adjusted)[0]![0]!)).toBe(3);
});

test("row insert above or below a merge leaves it alone", () => {
  const rows = [
    row("r1", spanned(2, 1, "a"), cell("b")),
    row("r2", cell(), cell("c")),
  ];
  for (const at of [0, 2]) {
    const adjusted = adjustSpansForRowInsert(rows, at);
    expect(cellRowspan(leafCellRows(adjusted)[0]![0]!)).toBe(2);
  }
});

test("row remove overlapping a merge shrinks it", () => {
  const rows = [
    row("r1", spanned(3, 1, "a"), cell("b")),
    row("r2", cell(), cell()),
    row("r3", cell(), cell("c")),
  ];
  const adjusted = adjustSpansForRowRemove(rows, 1, 1);
  expect(cellRowspan(leafCellRows(adjusted)[0]![0]!)).toBe(2);
});

test("span adjustments work through row groups", () => {
  const rows = [
    group(
      "g",
      row("r1", spanned(2, 1, "a"), cell("b")),
      row("r2", cell(), cell("c")),
    ),
  ];
  const adjusted = adjustSpansForRowInsert(rows, 1);
  expect(cellRowspan(leafCellRows(adjusted)[0]![0]!)).toBe(3);
});
