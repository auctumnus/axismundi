/// Mirrors src/controller/html/translations.rs:tokenize_preserving_whitespace —
/// splits text into runs of non-whitespace and records each run's [start, end)
/// character offsets so we can later splice IPA outputs back into the original.
const tokenize = (text: string): { tokens: string[]; ranges: [number, number][] } => {
  const tokens: string[] = [];
  const ranges: [number, number][] = [];
  let start: number | null = null;
  for (let i = 0; i < text.length; i++) {
    const isWs = /\s/.test(text[i]!);
    if (isWs) {
      if (start !== null) {
        tokens.push(text.slice(start, i));
        ranges.push([start, i]);
        start = null;
      }
    } else if (start === null) {
      start = i;
    }
  }
  if (start !== null) {
    tokens.push(text.slice(start));
    ranges.push([start, text.length]);
  }
  return { tokens, ranges };
};

const reassemble = (
  original: string,
  ranges: [number, number][],
  outputs: string[],
): string => {
  let out = "";
  let cursor = 0;
  for (let i = 0; i < ranges.length; i++) {
    const [s, e] = ranges[i]!;
    out += original.slice(cursor, s);
    out += outputs[i] ?? original.slice(s, e);
    cursor = e;
  }
  out += original.slice(cursor);
  return out;
};

const estimate = async (
  text: string,
  soundChangeSetId: string,
  signal?: AbortSignal,
): Promise<string> => {
  const { tokens, ranges } = tokenize(text);
  if (tokens.length === 0) return text;

  const response = await fetch(
    `/api/sound-change-sets/${soundChangeSetId}/run`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ input_words: tokens }),
      signal,
    },
  );

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(errorText || "Failed to estimate IPA");
  }

  const data = await response.json();
  if (Array.isArray(data.errors) && data.errors.length > 0) {
    const messages = data.errors
      .map((e: { message?: string }) => e.message ?? "")
      .filter(Boolean)
      .join(", ");
    throw new Error(`IPA estimation failed: ${messages}`);
  }

  return reassemble(text, ranges, data.outputWords ?? []);
};

const getOrCreateFieldErrors = (section: HTMLElement): HTMLUListElement => {
  let ul = section.querySelector<HTMLUListElement>("ul.field-errors");
  if (!ul) {
    ul = document.createElement("ul");
    ul.className = "field-errors";
    const labelRow = section.querySelector(".label-with-action");
    if (labelRow && labelRow.nextSibling) {
      section.insertBefore(ul, labelRow.nextSibling);
    } else {
      section.appendChild(ul);
    }
  }
  return ul;
};

const clearFieldErrors = (section: HTMLElement) => {
  const ul = section.querySelector("ul.field-errors");
  if (ul) ul.remove();
};

document.addEventListener("DOMContentLoaded", () => {
  const button = document.getElementById(
    "estimate-ipa-button",
  ) as HTMLButtonElement | null;
  if (!button) return;

  const soundChangeSetId = button.getAttribute("data-sound-change-set");
  if (!soundChangeSetId) return;

  // Resolve fresh at click time: on the edit page, the quotations editor
  // (mounted as a React tree on #quotations-editor-root) tears down the
  // server-rendered <textarea id="translated_text"> and renders a hidden,
  // controlled <input id="translated_text"> in its place. Any reference
  // captured at DOMContentLoaded may be either orphaned or replaced.
  const currentTextValue = (): string | null => {
    const el = document.getElementById("translated_text") as
      | HTMLInputElement
      | HTMLTextAreaElement
      | null;
    return el ? el.value : null;
  };
  const currentIpaInput = () =>
    document.getElementById("ipa") as HTMLTextAreaElement | null;
  const currentIpaSection = () =>
    currentIpaInput()?.closest("section") as HTMLElement | null;

  const setRunning = () => {
    button.disabled = true;
    button.ariaLabel = "Estimating IPA";
    button.classList.add("loading");
    const section = currentIpaSection();
    if (section) clearFieldErrors(section);
  };

  const setIdle = () => {
    button.disabled = false;
    button.ariaLabel = "Estimate IPA";
    button.classList.remove("loading");
  };

  const setErrored = (message: string) => {
    button.disabled = false;
    button.ariaLabel = "Error estimating IPA";
    button.classList.remove("loading");
    const section = currentIpaSection();
    if (section) {
      const ul = getOrCreateFieldErrors(section);
      ul.innerHTML = "";
      const li = document.createElement("li");
      li.textContent = message;
      ul.appendChild(li);
    }
  };

  let controller: AbortController | null = null;

  const run = async () => {
    const text = currentTextValue();
    const ipaInput = currentIpaInput();
    if (text === null || !ipaInput) return;
    if (controller) controller.abort();
    controller = new AbortController();
    const signal = controller.signal;
    try {
      setRunning();
      const [estimatedIpa] = await Promise.all([
        estimate(text, soundChangeSetId, signal),
        new Promise((resolve) => setTimeout(resolve, 500)),
      ]);
      ipaInput.value = estimatedIpa;
      setIdle();
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
      console.error("Error estimating IPA:", error);
      setErrored(
        error instanceof Error ? error.message : "Failed to estimate IPA",
      );
    }
  };

  button.addEventListener("click", (event) => {
    event.preventDefault();
    run();
  });
});
