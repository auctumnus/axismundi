import type { Cell, Row } from "./table";

// helpers for merged cells (rowspan/colspan). all coordinates here are
// leaf coordinates: (leaf row index, leaf column index)

export const cellRowspan = (cell: Cell): number => cell.rowspan ?? 1;
export const cellColspan = (cell: Cell): number => cell.colspan ?? 1;

// an inclusive rectangle of leaf coordinates
export interface Rect {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

/** The cells of each individual row, in visual order. */
export const leafCellRows = (rows: Row[]): Cell[][] => {
  const result: Cell[][] = [];
  const walk = (rows: Row[]) => {
    for (const row of rows) {
      if (row.type === "Group") {
        walk(row.rows);
      } else {
        result.push(row.cells);
      }
    }
  };
  walk(rows);
  return result;
};

/**
 * Maps every leaf cell through fn, preserving the row tree structure.
 * fn receives the cell and its leaf coordinates.
 */
export const mapLeafCells = (
  rows: Row[],
  fn: (cell: Cell, r: number, c: number) => Cell,
): Row[] => {
  let r = 0;
  const walk = (rows: Row[]): Row[] =>
    rows.map((row) => {
      if (row.type === "Group") {
        return { ...row, rows: walk(row.rows) };
      }
      const mapped = {
        ...row,
        cells: row.cells.map((cell, c) => fn(cell, r, c)),
      };
      r += 1;
      return mapped;
    });
  return walk(rows);
};

/** A copy of the cell with the given spans, omitting the properties when 1. */
export const withSpan = (
  cell: Cell,
  rowspan: number,
  colspan: number,
): Cell => {
  const next: Cell = { phonemes: cell.phonemes };
  if (rowspan > 1) next.rowspan = rowspan;
  if (colspan > 1) next.colspan = colspan;
  return next;
};

const key = (r: number, c: number) => `${r},${c}`;

/**
 * Positions hidden underneath a merged cell, mapped to the leaf coordinates
 * of the anchor (top-left) cell covering them. Anchors themselves are not
 * included.
 */
export const coveredCells = (rows: Row[]): Map<string, [number, number]> => {
  const covered = new Map<string, [number, number]>();
  leafCellRows(rows).forEach((cells, r) => {
    cells.forEach((cell, c) => {
      if (covered.has(key(r, c))) return;
      const rs = cellRowspan(cell);
      const cs = cellColspan(cell);
      for (let dr = 0; dr < rs; dr++) {
        for (let dc = 0; dc < cs; dc++) {
          if (dr !== 0 || dc !== 0) covered.set(key(r + dr, c + dc), [r, c]);
        }
      }
    });
  });
  return covered;
};

export const isCovered = (
  covered: Map<string, [number, number]>,
  r: number,
  c: number,
): boolean => covered.has(key(r, c));

/** Resolves a position to the anchor of the merged region containing it. */
export const anchorOf = (
  covered: Map<string, [number, number]>,
  r: number,
  c: number,
): [number, number] => covered.get(key(r, c)) ?? [r, c];

/** The full rectangle of the merged region containing (r, c). */
export const regionOf = (
  grid: Cell[][],
  covered: Map<string, [number, number]>,
  r: number,
  c: number,
): Rect => {
  const [ar, ac] = anchorOf(covered, r, c);
  const anchor = grid[ar]?.[ac];
  if (!anchor) {
    return { top: r, left: c, bottom: r, right: c };
  }
  return {
    top: ar,
    left: ac,
    bottom: ar + cellRowspan(anchor) - 1,
    right: ac + cellColspan(anchor) - 1,
  };
};

const union = (a: Rect, b: Rect): Rect => ({
  top: Math.min(a.top, b.top),
  left: Math.min(a.left, b.left),
  bottom: Math.max(a.bottom, b.bottom),
  right: Math.max(a.right, b.right),
});

const intersects = (a: Rect, b: Rect): boolean =>
  a.left <= b.right && b.left <= a.right && a.top <= b.bottom && b.top <= a.bottom;

const contains = (outer: Rect, inner: Rect): boolean =>
  outer.top <= inner.top &&
  outer.left <= inner.left &&
  outer.bottom >= inner.bottom &&
  outer.right >= inner.right;

/**
 * The smallest rectangle containing both cells' merged regions, grown until
 * no existing merged region partially sticks out of it.
 */
export const mergeRect = (
  rows: Row[],
  a: [number, number],
  b: [number, number],
): Rect => {
  const grid = leafCellRows(rows);
  const covered = coveredCells(rows);
  let rect = union(
    regionOf(grid, covered, a[0], a[1]),
    regionOf(grid, covered, b[0], b[1]),
  );
  let changed = true;
  while (changed) {
    changed = false;
    grid.forEach((cells, r) => {
      cells.forEach((cell, c) => {
        if (isCovered(covered, r, c)) return;
        if (cellRowspan(cell) === 1 && cellColspan(cell) === 1) return;
        const region = regionOf(grid, covered, r, c);
        if (intersects(rect, region) && !contains(rect, region)) {
          rect = union(rect, region);
          changed = true;
        }
      });
    });
  }
  return rect;
};

// spreadsheet-style span maintenance for structural edits. each of these must
// be called BEFORE the corresponding cells/rows are spliced in or out, since
// the conditions are expressed in pre-edit leaf coordinates.

/** A column is about to be inserted at colIndex: widen merges it lands inside. */
export const adjustSpansForColumnInsert = (
  rows: Row[],
  colIndex: number,
): Row[] =>
  mapLeafCells(rows, (cell, _r, c) => {
    const cs = cellColspan(cell);
    if (c < colIndex && colIndex < c + cs) {
      return withSpan(cell, cellRowspan(cell), cs + 1);
    }
    return cell;
  });

/** Columns [colIndex, colIndex + count) are about to be removed: shrink merges. */
export const adjustSpansForColumnRemove = (
  rows: Row[],
  colIndex: number,
  count: number,
): Row[] =>
  mapLeafCells(rows, (cell, _r, c) => {
    if (c >= colIndex) return cell;
    const cs = cellColspan(cell);
    const overlap = Math.max(0, Math.min(c + cs, colIndex + count) - colIndex);
    if (overlap > 0) {
      return withSpan(cell, cellRowspan(cell), cs - overlap);
    }
    return cell;
  });

/** A row is about to be inserted at leaf index rowIndex: stretch merges it lands inside. */
export const adjustSpansForRowInsert = (rows: Row[], rowIndex: number): Row[] =>
  mapLeafCells(rows, (cell, r, _c) => {
    const rs = cellRowspan(cell);
    if (r < rowIndex && rowIndex < r + rs) {
      return withSpan(cell, rs + 1, cellColspan(cell));
    }
    return cell;
  });

/** Leaf rows [rowIndex, rowIndex + count) are about to be removed: shrink merges. */
export const adjustSpansForRowRemove = (
  rows: Row[],
  rowIndex: number,
  count: number,
): Row[] =>
  mapLeafCells(rows, (cell, r, _c) => {
    if (r >= rowIndex) return cell;
    const rs = cellRowspan(cell);
    const overlap = Math.max(0, Math.min(r + rs, rowIndex + count) - rowIndex);
    if (overlap > 0) {
      return withSpan(cell, rs - overlap, cellColspan(cell));
    }
    return cell;
  });
