import assert from "node:assert/strict";
import { resolve } from "node:path";
import { chromium } from "playwright";
import { expect } from "playwright/test";
import { bundleAsync } from "lightningcss";
import { PRESETS } from "../src/page/languages/phonology-editor/state";

const frontend = resolve(import.meta.dir, "..");
const bundle = await Bun.build({
  entrypoints: ["phonology-editor.tsx", "grammar-table-editor.tsx"].map(
    (name) => resolve(frontend, `src/page/languages/${name}`),
  ),
  target: "browser",
  format: "esm",
});
assert(bundle.success, String(bundle.logs));
const assets = new Map<string, Blob | string>();
for (const output of bundle.outputs)
  assets.set(`/${output.path.split("/").at(-1)}`, output);
for (const [url, path] of [
  ["/main.css", "src/main.css"],
  ["/table.css", "src/components/phonology-table.css"],
  ["/modal.css", "src/components/modal/modal.css"],
  ["/tooltip.css", "src/components/tooltip/tooltip.css"],
  ["/word-combobox.css", "src/components/combobox/word-combobox.css"],
  ["/async-select.css", "src/components/async-select/async-select.css"],
  ["/sound-changer.css", "src/page/sound-changes/runner/sound-changer.css"],
]) {
  assets.set(
    url!,
    new TextDecoder().decode(
      (await bundleAsync({ filename: resolve(frontend, path!) })).code,
    ),
  );
}
const icons =
  (await Bun.file(resolve(frontend, "../templates/layout.html")).text())
    .match(/<symbol[\s\S]*?<\/symbol>/g)
    ?.join("") ?? "";
const phonologyInitial = PRESETS["Default"]!;
const phonologyImported = structuredClone(PRESETS["Estonian Consonants"]!);
if (phonologyImported.rows[0]!.type !== "Individual")
  throw new Error("Invalid fixture");
phonologyImported.rows[0]!.cells[0]!.colspan = 2;
phonologyImported.rows[0]!.cells[1] = { phonemes: [] };
const grammarInitial = {
  columns: [{ type: "Individual", heading: "Number" }],
  rows: [
    {
      type: "Individual",
      heading: "Case",
      cells: [{ changes: "original:\n  a => e" }],
    },
  ],
  preamble: "Class V {a}",
};
const grammarImported = {
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
  preamble: "Class V {a, e, i}",
};
const grid = (value: unknown) => {
  const { preamble, ...body } = value as Record<string, unknown>;
  return body;
};
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  fetch(request) {
    const url = new URL(request.url);
    const path = url.pathname;
    const grammar = url.searchParams.has("grammar");
    const asset = assets.get(path);
    if (asset)
      return new Response(asset, {
        headers: {
          "Content-Type": path.endsWith(".css")
            ? "text/css"
            : "application/javascript",
        },
      });
    return new Response(
      `<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1">
      <link rel="stylesheet" href="/main.css"><link rel="stylesheet" href="/table.css">
      <link rel="stylesheet" href="/modal.css"><link rel="stylesheet" href="/tooltip.css">
      ${grammar ? '<link rel="stylesheet" href="/word-combobox.css"><link rel="stylesheet" href="/async-select.css"><link rel="stylesheet" href="/sound-changer.css">' : ""}</head>
      <body><svg style="display:none" aria-hidden="true">${icons}</svg><main><h1>edit ${grammar ? "grammar" : "phonology"} table body</h1><form class="default" id="form">${grammar ? '<input id="table-editor" type="hidden">' : '<div id="mount"></div>'}
      <button type="submit">Next</button></form></main>
      ${
        grammar
          ? `<script type="application/json" id="initial-grammar-table-body">${JSON.stringify(grid(grammarInitial))}</script>
      <script type="application/json" id="initial-grammar-table-preamble">${JSON.stringify(grammarInitial.preamble)}</script>
      <script type="application/json" id="grammar-table-editor-options">{"previewUrl":"/preview","hasIpaEstimator":false,"languageCode":"test","name":"Declension"}</script>
      <script type="module" src="/grammar-table-editor.js"></script>`
          : `<script type="module">
      import {mountPhonologyEditor} from '/phonology-editor.js';
      mountPhonologyEditor('mount', ${JSON.stringify(phonologyInitial)}, 'Consonants');</script>`
      }
      <script>
      window.submissions=[];document.getElementById('form').addEventListener('submit',event=>{
        event.preventDefault();window.submissions.push(Object.fromEntries(new FormData(event.target).entries()));
      });</script></body></html>`,
      { headers: { "Content-Type": "text/html" } },
    );
  },
});

try {
  const browser = await chromium.launch({
    executablePath: process.env.PHONOLOGY_JSON_CHROMIUM_EXECUTABLE,
  });
  try {
    for (const grammar of [false, true]) {
      const initial = grammar ? grammarInitial : phonologyInitial;
      const imported = grammar ? grammarImported : phonologyImported;
      const importFilename = grammar ? "declension.json" : "consonants.json";
      const page = await browser.newPage({
        viewport: { width: 1000, height: 900 },
      });
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(`${server.url}${grammar ? "?grammar" : ""}`);
      const bodyField = page.locator('input[name="body"]');
      const data = async () => ({
        ...JSON.parse(await bodyField.inputValue()),
        ...(grammar
          ? {
              preamble: await page
                .locator('input[name="preamble"]')
                .inputValue(),
            }
          : {}),
      });
      const preambleIs = async (value: unknown) => {
        if (grammar)
          await expect(
            page.getByRole("textbox", {
              name: "Shared sound changes",
              exact: true,
            }),
          ).toHaveText((value as typeof grammarInitial).preamble);
      };
      await expect(bodyField).toHaveValue(JSON.stringify(grid(initial)));
      await preambleIs(initial);
      const downloadFile = async () => {
        const pending = page.waitForEvent("download");
        await page
          .getByRole("button", { name: "Export JSON", exact: true })
          .click();
        const download = await pending;
        assert.equal(
          download.suggestedFilename(),
          grammar ? "Declension.json" : "Consonants.json",
        );
        const path = await download.path();
        assert(path);
        return await Bun.file(path).text();
      };
      assert.deepEqual(JSON.parse(await downloadFile()), initial);

      const upload = async (name: string, contents: string) => {
        const pending = page.waitForEvent("filechooser");
        await page
          .getByRole("button", { name: "Import JSON", exact: true })
          .click();
        await (
          await pending
        ).setFiles({
          name,
          mimeType: "application/json",
          buffer: Buffer.from(contents),
        });
      };
      await upload(importFilename, JSON.stringify(imported));
      await expect(page.getByRole("status")).toContainText(
        `Imported ${importFilename}`,
      );
      assert.deepEqual(await data(), imported);
      await preambleIs(imported);
      await expect(page.locator('td[colspan="2"]')).toHaveCount(1);
      const exported = await downloadFile();
      assert.deepEqual(JSON.parse(exported), imported);
      await page.getByRole("button", { name: "Undo", exact: true }).click();
      assert.deepEqual(await data(), initial);
      await preambleIs(initial);
      await page.getByRole("button", { name: "Redo", exact: true }).click();
      assert.deepEqual(await data(), imported);
      await preambleIs(imported);

      await upload("broken.json", "{bad json");
      await expect(page.getByRole("alert")).toContainText("not valid JSON");
      assert.deepEqual(await data(), imported);
      await preambleIs(imported);
      await upload(
        "wrong-shape.json",
        grammar
          ? '{"rows":[],"columns":[],"preamble":null}'
          : '{"rows":[],"columns":[],"annotations":null}',
      );
      await expect(page.getByRole("alert")).toContainText(
        grammar
          ? "preamble: expected a string"
          : "annotations: expected an array",
      );
      assert.deepEqual(await data(), imported);
      await preambleIs(imported);
      // Re-select the same file after an error, and import the actual downloaded JSON.
      await upload(importFilename, exported);
      await expect(page.getByRole("alert")).toHaveCount(0);
      await expect(page.getByRole("status")).toContainText(
        `Imported ${importFilename}`,
      );
      assert.deepEqual(await data(), imported);
      await preambleIs(imported);
      assert.deepEqual(
        await page.evaluate(() => (window as any).submissions),
        [],
      );
      await page.getByRole("button", { name: "Next", exact: true }).click();
      const submissions = await page.evaluate(
        () => (window as any).submissions,
      );
      assert.deepEqual(
        submissions.map((value: any) => ({
          ...JSON.parse(value.body),
          ...(grammar ? { preamble: value.preamble } : {}),
        })),
        [imported],
      );
      assert.deepEqual(errors, []);
      await page.screenshot({
        path: `/tmp/axismundi-${grammar ? "grammar" : "phonology"}-json.png`,
        fullPage: true,
      });
      console.log(
        `${grammar ? "Grammar" : "Phonology"} JSON browser checks passed: file round trip, validation, history, and form submission.`,
      );
      await page.close();
    }
  } finally {
    await browser.close();
  }
} finally {
  server.stop(true);
}
