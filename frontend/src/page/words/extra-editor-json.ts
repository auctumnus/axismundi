import type { JsonValue } from "./extra-editor-tree";

export function parseJson(text: string): JsonValue {
  return JSON.parse(text, (_key, value: unknown) => {
    if (typeof value === "number" && !Number.isFinite(value))
      throw new Error("JSON numbers must be finite");
    return value;
  }) as JsonValue;
}

// Compare decimal values without first converting the source token to a float.
// Formatting differences (1.0, 10e-1) are harmless; rounded digits are not.
function decimalValue(token: string): string {
  const [, sign, integer, fraction = "", exponent = "0"] = token.match(
    /^(-?)(\d+)(?:\.(\d+))?(?:[eE]([+-]?\d+))?$/,
  )!;
  const digits = (integer! + fraction).replace(/^0+/, "");
  if (!digits) return `${sign}0`;
  const coefficient = digits.replace(/0+$/, "");
  const power =
    BigInt(exponent) -
    BigInt(fraction.length) +
    BigInt(digits.length - coefficient.length);
  return `${sign}${coefficient}e${power}`;
}

function preservesNumber(token: string, value: number): boolean {
  return decimalValue(token) === decimalValue(JSON.stringify(value));
}

export function parseStructuredNumber(text: string): number {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("Enter a valid JSON number.");
  }
  if (typeof value !== "number" || !Number.isFinite(value))
    throw new Error("Enter a finite JSON number.");
  if (!preservesNumber(text.trim(), value))
    throw new Error(
      "This number needs more precision than this field supports.",
    );
  return value;
}

export function parseStructuredJson(text: string): JsonValue {
  const value = parseJson(text);
  // Match entire string tokens so digits and escaped quotes inside strings
  // cannot be mistaken for number tokens. parseJson has checked the syntax.
  const tokens = text.matchAll(
    /"(?:[^"\\]|\\[\s\S])*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/g,
  );
  for (const [token] of tokens) {
    if (token.startsWith('"')) {
      // Native single-line inputs strip both CR and LF from values and keys.
      if (/[\r\n]/.test(JSON.parse(token) as string))
        throw new Error("Line breaks require the raw JSON editor.");
    } else if (!preservesNumber(token, Number(token))) {
      throw new Error("This number requires the raw JSON editor.");
    }
  }
  return value;
}
