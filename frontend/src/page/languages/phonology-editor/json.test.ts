import { expect, test } from "bun:test";
import { parseTableBody, serializeTableBody } from "./json";
import { apply, initialState, PRESETS } from "./state";
import type { Body } from "./table";

const merged = (): Body => ({
  columns: [
    {
      type: "Group",
      heading: "Place",
      columns: [
        { type: "Individual", heading: "Labial" },
        { type: "Individual", heading: "Alveolar" },
      ],
    },
  ],
  rows: [
    {
      type: "Group",
      heading: "Manner",
      rows: [
        {
          type: "Individual",
          heading: "Plosive",
          cells: [
            {
              phonemes: [{ text: "pʰ", annotations: [0] }],
              rowspan: 2,
              colspan: 2,
            },
            { phonemes: [] },
          ],
        },
        {
          type: "Individual",
          heading: "Fricative",
          cells: [{ phonemes: [] }, { phonemes: [] }],
        },
      ],
    },
  ],
  annotations: ["Aspirated <phoneme>"],
});

test("JSON round trips every preset and merged grouped tables", () => {
  for (const body of [...Object.values(PRESETS), merged()]) {
    const exported = serializeTableBody(body);
    expect(JSON.parse(exported)).toEqual(body);
    expect(JSON.parse(serializeTableBody(parseTableBody(exported)))).toEqual(
      body,
    );
  }
});

test("accepts a UTF-8 BOM and empty table bodies", () => {
  const body = { rows: [], columns: [], annotations: [] };
  expect(parseTableBody("\uFEFF" + JSON.stringify(body))).toEqual(body);
});

test("rejects malformed JSON and incorrectly typed nested values", () => {
  for (const value of [null, [], {}, { ...merged(), annotations: [1] }])
    expect(() => parseTableBody(JSON.stringify(value))).toThrow();
  expect(() => parseTableBody("{broken")).toThrow("not valid JSON");
  const body = merged();
  body.columns = [{ type: "Individual", heading: 3 } as any];
  expect(() => parseTableBody(JSON.stringify(body))).toThrow(
    "expected a string",
  );
});

test("rejects wrong row widths, empty phonemes and invalid annotation links", () => {
  const body = {
    columns: [{ type: "Individual", heading: "Front" }],
    rows: [
      {
        type: "Individual",
        heading: "High",
        cells: [{ phonemes: [] as any[] }],
      },
    ],
    annotations: ["Note"],
  };
  body.rows[0]!.cells.push({ phonemes: [] });
  expect(() => parseTableBody(JSON.stringify(body))).toThrow(
    "expected 1 cells",
  );
  body.rows[0]!.cells.pop();
  for (const phoneme of [
    { text: " ", annotations: [] },
    { text: "i", annotations: [1] },
    { text: "i", annotations: [-1] },
    { text: "i", annotations: [0.5] },
    { text: "i", annotations: "0" },
  ]) {
    body.rows[0]!.cells[0]!.phonemes = [phoneme];
    expect(() => parseTableBody(JSON.stringify(body))).toThrow();
  }
});

test("rejects invalid spans, overlapping merges and occupied covered cells", () => {
  for (const span of [0, -1, 1.5, 0x100000000, null, "2", 3]) {
    const body = merged();
    if (
      body.rows[0]!.type !== "Group" ||
      body.rows[0]!.rows[0]!.type !== "Individual"
    )
      throw new Error("Invalid fixture");
    body.rows[0]!.rows[0]!.cells[0]!.rowspan = span as any;
    expect(() => parseTableBody(JSON.stringify(body))).toThrow();
  }
  for (const cell of [
    { phonemes: [], colspan: 2 },
    { phonemes: [{ text: "s", annotations: [] }] },
  ]) {
    const body = merged();
    if (
      body.rows[0]!.type !== "Group" ||
      body.rows[0]!.rows[1]!.type !== "Individual"
    )
      throw new Error("Invalid fixture");
    body.rows[0]!.rows[1]!.cells[0] = cell;
    expect(() => parseTableBody(JSON.stringify(body))).toThrow();
  }
});

test("rejects a span crossing an earlier merged region", () => {
  const body: Body = {
    columns: [
      { type: "Individual", heading: "A" },
      { type: "Individual", heading: "B" },
    ],
    rows: [
      {
        type: "Individual",
        heading: "One",
        cells: [{ phonemes: [] }, { phonemes: [], rowspan: 2 }],
      },
      {
        type: "Individual",
        heading: "Two",
        cells: [{ phonemes: [], colspan: 2 }, { phonemes: [] }],
      },
    ],
    annotations: [],
  };
  expect(() => parseTableBody(JSON.stringify(body))).toThrow("cannot overlap");
});

test("import resets stale editor state and can be undone and redone", () => {
  const original = PRESETS["Default"]!;
  const before = {
    ...initialState(original, "Consonants"),
    select: { type: "Cell" as const, rowPath: [2], colPath: [2] },
    pendingModal: "EditPhoneme" as const,
    pendingPhonemeIndex: 2,
    keybindState: "Phoneme" as const,
  };
  const imported = parseTableBody(serializeTableBody(merged()));
  const after = apply(before, { type: "ImportBody", body: imported });
  expect(after.body).toEqual(imported);
  expect(after.name).toBe("Consonants");
  expect(after.focus).toEqual({ type: "TopLeft" });
  expect(after.select).toBeNull();
  expect(after.pendingModal).toBeNull();
  expect(after.pendingPhonemeIndex).toBeNull();
  expect(after.keybindState).toBe("Idle");
  const undone = apply(after, { type: "Undo" });
  expect(undone.body).toEqual(original);
  expect(apply(undone, { type: "Redo" }).body).toEqual(imported);
});
