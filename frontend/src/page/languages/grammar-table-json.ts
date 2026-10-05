import {
  flatRows,
  leafCount,
  type Body,
  type GridCell,
} from "./table-editor-core";
import {
  invalid,
  object,
  parseGridBody,
  parseJson,
  serializeJson,
  string,
} from "./table-json";

export type GrammarCell = GridCell & { changes: string };
export type GrammarBody = Body<GrammarCell>;
export type GrammarFile = { body: GrammarBody; preamble: string };
const bytes = (value: string) => new TextEncoder().encode(value).length;

// Files contain the grid plus its shared rules. Raw grid bodies are also accepted.
export const parseGrammarFile = (
  source: string,
  currentPreamble: string,
): GrammarFile => {
  const value = object(parseJson(source), "Grammar table");
  const preamble =
    value.preamble === undefined
      ? currentPreamble
      : string(value.preamble, "preamble");
  if (bytes(preamble) > 32 * 1024)
    return invalid("preamble", "shared rules must be at most 32 KiB.");
  const body = parseGridBody<GrammarCell>(value, {
    maxDepth: 6,
    parseCell: (cell, path) => {
      // Never silently treat a phonology file as an empty grammar table.
      if ("phonemes" in cell)
        return invalid(path, "expected grammar cell rules, found phonemes.");
      const changes =
        cell.changes === undefined
          ? ""
          : string(cell.changes, `${path}.changes`);
      if (bytes(changes) > 16 * 1024)
        return invalid(path, "cell rules must be at most 16 KiB.");
      return { changes };
    },
    isEmpty: (cell) => !cell.changes.trim(),
  });
  const rows = flatRows(body.rows);
  const columns = leafCount(body.columns);
  if (!rows.length || !columns)
    return invalid(
      "Grammar table",
      "at least one row and column are required.",
    );
  if (rows.length > 50 || columns > 50)
    return invalid(
      "Grammar table",
      "at most 50 rows and 50 columns are allowed.",
    );
  const programs = new Set<string>();
  const occupied = new Set<string>();
  rows.forEach(({ row }, r) =>
    row.cells.forEach((cell, c) => {
      if (occupied.has(`${r},${c}`)) return;
      const program = [preamble.trim(), cell.changes.trim()]
        .filter(Boolean)
        .join("\n");
      programs.add(program);
      for (let dr = 0; dr < (cell.rowspan ?? 1); dr++)
        for (let dc = 0; dc < (cell.colspan ?? 1); dc++)
          occupied.add(`${r + dr},${c + dc}`);
    }),
  );
  if (programs.size > 256)
    return invalid(
      "Grammar table",
      "at most 256 unique rule programs are allowed.",
    );
  if (
    [...programs].reduce((sum, program) => sum + bytes(program), 0) >
    256 * 1024
  )
    return invalid("Grammar table", "rule programs may total at most 256 KiB.");
  return { body, preamble };
};

export const serializeGrammarFile = ({ body, preamble }: GrammarFile): string =>
  serializeJson({ ...body, preamble });
