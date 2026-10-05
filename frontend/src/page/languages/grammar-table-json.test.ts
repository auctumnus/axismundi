import { expect, test } from "bun:test";
import {
  parseGrammarFile,
  serializeGrammarFile,
  type GrammarFile,
  type GrammarCell,
} from "./grammar-table-json";
import { applyGrammar, initialGrammarState } from "./grammar-table-state";
import type { Row } from "./table-editor-core";

const fixture = (): GrammarFile => ({
  preamble: "Class vowel {a, e, i, o, u}\n",
  body: {
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
            cells: [
              { changes: "plural:\n  * => s / $", rowspan: 2, colspan: 2 },
              { changes: "" },
            ],
          },
          {
            type: "Individual",
            heading: "Accusative",
            cells: [{ changes: "" }, { changes: "" }],
          },
        ],
      },
    ],
  },
});
const simple = (changes = ""): GrammarFile => ({
  body: {
    columns: [{ type: "Individual", heading: "A" }],
    rows: [{ type: "Individual", heading: "B", cells: [{ changes }] }],
  },
  preamble: "",
});
const parse = (file: GrammarFile) =>
  parseGrammarFile(serializeGrammarFile(file), "previous");

test("grammar JSON preserves grouped headings, merges, cell rules and shared rules", () => {
  const file = fixture();
  expect(JSON.parse(serializeGrammarFile(parse(file)))).toEqual({
    ...file.body,
    preamble: file.preamble,
  });
});

test("raw grammar bodies preserve the current preamble and default empty rules", () => {
  const body = {
    columns: [{ type: "Individual", heading: "A" }],
    rows: [{ type: "Individual", heading: "B", cells: [{}] }],
  };
  const imported = parseGrammarFile(JSON.stringify(body), "Class V {a}");
  expect(imported.preamble).toBe("Class V {a}");
  expect(
    imported.body.rows[0]!.type === "Individual" &&
      imported.body.rows[0]!.cells[0]!.changes,
  ).toBe("");
});

test("rejects wrong file types, invalid cell data, grids and merges", () => {
  for (const value of [
    "{broken",
    "null",
    "{}",
    JSON.stringify({ ...simple().body, preamble: 1 }),
    JSON.stringify({ rows: [], columns: [] }),
  ])
    expect(() => parseGrammarFile(value, "")).toThrow();
  for (const cell of [
    { changes: 123 },
    { phonemes: [] },
    { changes: "", rowspan: 0 },
    { changes: "", colspan: 2 },
  ]) {
    const file = simple();
    (file.body.rows[0] as any).cells = [cell];
    expect(() => parse(file)).toThrow();
  }
  const file = fixture();
  (file.body.rows[0] as any).rows[1].cells[0].changes = "hidden rules";
  expect(() => parse(file)).toThrow("must be empty");
});

test("enforces UTF-8 rule sizes and server grid limits", () => {
  expect(() => parse(simple("ə".repeat(8193)))).toThrow("16 KiB");
  expect(() => parse({ ...simple(), preamble: "ə".repeat(16385) })).toThrow(
    "32 KiB",
  );
  const file = simple();
  file.body.rows = Array.from({ length: 51 }, () => ({
    type: "Individual",
    heading: "A",
    cells: [{ changes: "" }],
  }));
  expect(() => parse(file)).toThrow("50 rows");
  let row: Row<GrammarCell> = {
    type: "Individual",
    heading: "Leaf",
    cells: [{ changes: "" }],
  };
  for (let i = 0; i < 6; i++)
    row = { type: "Group", heading: "Group", rows: [row] };
  file.body.rows = [row];
  expect(() => parse(file)).toThrow("nested too deeply");
});

test("limits unique composed programs and their combined byte size", () => {
  const file = simple();
  file.body.columns = Array.from({ length: 17 }, () => ({
    type: "Individual",
    heading: "A",
  }));
  file.body.rows = Array.from({ length: 17 }, (_, r) => ({
    type: "Individual",
    heading: "B",
    cells: Array.from({ length: 17 }, (_, c) => ({ changes: `${r},${c}` })),
  }));
  expect(() => parse(file)).toThrow("256 unique");
  file.body.rows = file.body.rows.slice(0, 1);
  (file.body.rows[0] as any).cells = Array.from({ length: 17 }, (_, c) => ({
    changes: `${c}` + "x".repeat(16000),
  }));
  expect(() => parse(file)).toThrow("256 KiB");
});

test("history restores grid and preamble together across import, edits and redo", () => {
  const original = simple("original");
  let state = initialGrammarState(original.body, "original shared rules");
  state = applyGrammar(state, {
    type: "Select",
    select: { type: "Cell", row: 0, column: 0 },
  });
  const before = state;
  const file = parse(fixture());
  state = applyGrammar(state, { type: "Import", file });
  expect(state.body).toEqual(file.body);
  expect(state.preamble).toBe(file.preamble);
  expect(state.focus).toEqual({ type: "TopLeft" });
  expect(state.select).toBeNull();
  state = applyGrammar(state, { type: "Undo" });
  expect(state.body).toEqual(before.body);
  expect(state.preamble).toBe(before.preamble);
  state = applyGrammar(state, { type: "Redo" });
  expect(state.preamble).toBe(file.preamble);
  state = applyGrammar(state, {
    type: "SetPreamble",
    preamble: "edited shared rules",
  });
  state = applyGrammar(state, {
    type: "SetCell",
    row: 0,
    column: 0,
    cell: { changes: "edited cell" },
  });
  state = applyGrammar(state, { type: "Undo" });
  expect(state.preamble).toBe("edited shared rules");
  state = applyGrammar(state, { type: "Undo" });
  expect(state.preamble).toBe(file.preamble);
  state = applyGrammar(state, { type: "Undo" });
  expect(state.body).toEqual(original.body);
  expect(state.preamble).toBe(before.preamble);
});
