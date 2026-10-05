import {
  array,
  invalid,
  object,
  parseGridBody,
  parseJson,
  serializeJson,
  string,
} from "../table-json";
import type { Body, Cell, Phoneme } from "./table";

export const parseTableBody = (source: string): Body => {
  const value = object(parseJson(source), "Table body");
  const annotations = array(value.annotations, "annotations").map((value, i) =>
    string(value, `annotations[${i}]`),
  );
  const phoneme = (value: unknown, path: string): Phoneme => {
    const item = object(value, path);
    const text = string(item.text, `${path}.text`);
    if (!text.trim()) return invalid(path, "phoneme text cannot be empty.");
    const links = array(item.annotations, `${path}.annotations`).map(
      (value) => {
        if (
          typeof value !== "number" ||
          !Number.isInteger(value) ||
          value < 0 ||
          value >= annotations.length
        )
          return invalid(path, "annotation index is out of bounds.");
        return value;
      },
    );
    return { text, annotations: links };
  };
  const body = parseGridBody<Cell>(value, {
    parseCell: (cell, path) => ({
      phonemes: array(cell.phonemes, `${path}.phonemes`).map((value, k) =>
        phoneme(value, `${path}.phonemes[${k}]`),
      ),
    }),
    isEmpty: (cell) => cell.phonemes.length === 0,
  });
  return { ...body, annotations };
};

export const serializeTableBody = (body: Body): string => serializeJson(body);
