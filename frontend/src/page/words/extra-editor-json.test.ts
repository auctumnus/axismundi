import { expect, test } from "bun:test";
import {
  parseJson,
  parseStructuredJson,
  parseStructuredNumber,
} from "./extra-editor-json";

test("structured JSON accepts equivalent decimal formats and digits inside strings", () => {
  const raw =
    '{"9007199254740993": [1.0, 10e-1, 1e20, 0.1, "9007199254740993", "\\\"123", "literal\\\\n"]}';
  expect(parseStructuredJson(raw)).toEqual(JSON.parse(raw));
  for (const raw of ["1.0", "10e-1", " 0.10 ", "1e20", "9007199254740992"])
    expect(parseStructuredNumber(raw)).toBe(Number(raw));
});

test("line breaks in values and keys require lossless raw editing", () => {
  for (const raw of [
    '{"s":"line1\\nline2"}',
    '{"a\\nb":"value"}',
    '["line1\\rline2"]',
    '{"a\\r\\nb":"value"}',
    '"\\u000a"',
  ]) {
    expect(() => parseStructuredJson(raw)).toThrow("raw JSON editor");
    expect(parseJson(raw)).toEqual(JSON.parse(raw));
  }
});

test("numbers that round during serialization require lossless raw editing", () => {
  for (const number of [
    "9007199254740993",
    "18446744073709551615",
    "-9007199254740993",
    "0.10000000000000001",
    "1e-999",
    "-0",
  ]) {
    expect(() => parseStructuredJson(`{"nested":[${number}]}`)).toThrow(
      "raw JSON editor",
    );
    expect(() => parseStructuredNumber(number)).toThrow("precision");
    expect(parseJson(number)).toBe(Number(number));
  }
});

test("raw editing still rejects invalid JSON and overflowing numbers", () => {
  for (const raw of ["broken", "1e999", '{"n":1e999}'])
    expect(() => parseJson(raw)).toThrow();
  for (const raw of ["12oops", "null", "true", "1e999"])
    expect(() => parseStructuredNumber(raw)).toThrow();
});
