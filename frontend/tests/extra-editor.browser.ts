import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { chromium, firefox, type Locator, type Page } from "playwright";
import { expect } from "playwright/test";
import { bundleAsync } from "lightningcss";

const engines = (process.env.EXTRA_EDITOR_BROWSERS ?? "chromium").split(",");
const repeats = Number(process.env.EXTRA_EDITOR_REPEATS ?? "1");
assert(
  Number.isInteger(repeats) && repeats > 0,
  "EXTRA_EDITOR_REPEATS must be a positive integer",
);
for (const name of engines)
  assert(
    name === "chromium" || name === "firefox",
    "Supported engines: chromium,firefox",
  );
// Keep each engine's browser pipes and fixture server in its own process.
// Reusing a Bun process across Chromium and Firefox can stall after Chromium
// finishes, even though either engine passes in isolation.
if (engines.length > 1 || repeats > 1) {
  for (let run = 1; run <= repeats; run++) {
    if (repeats > 1) console.log(`Browser suite repetition ${run}/${repeats}`);
    for (const name of engines) {
      const child = Bun.spawn([process.execPath, import.meta.filename], {
        env: {
          ...process.env,
          EXTRA_EDITOR_BROWSERS: name,
          EXTRA_EDITOR_REPEATS: "1",
        },
        stdin: "inherit",
        stdout: "inherit",
        stderr: "inherit",
      });
      const code = await child.exited;
      if (code !== 0) process.exit(code);
    }
  }
  process.exit(0);
}
const timeout = setTimeout(() => {
  console.error(`${engines[0]}: extra editor browser checks exceeded 120s`);
  process.exit(1);
}, 120_000);

// A real form fixture: exercise the public mounts, native validation and the
// site's styles without requiring authentication or the backend services.
const frontend = resolve(import.meta.dir, "..");
const bundles = await Bun.build({
  entrypoints: ["extra-editor.tsx", "definitions-editor.tsx", "edit.ts"].map(
    (name) => resolve(frontend, `src/page/words/${name}`),
  ),
  target: "browser",
  format: "esm",
});
assert(bundles.success, String(bundles.logs));
const icons =
  (await Bun.file(resolve(frontend, "../templates/layout.html")).text())
    .match(/<symbol[\s\S]*?<\/symbol>/g)
    ?.join("") ?? "";
const assets = new Map<string, Blob | string>();
for (const output of bundles.outputs)
  assets.set(`/${output.path.split("/").at(-1)}`, output);
for (const [url, path] of [
  ["/main.css", "src/main.css"],
  ["/editor.css", "src/page/words/extra-editor.css"],
  ["/definitions.css", "src/page/words/edit.css"],
  ["/modal.css", "src/components/modal/modal.css"],
  ["/tooltip.css", "src/components/tooltip/tooltip.css"],
]) {
  assets.set(
    url!,
    new TextDecoder().decode(
      (await bundleAsync({ filename: resolve(frontend, path!) })).code,
    ),
  );
}
const server = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/estimate-ipa") {
      const fields = await request.formData();
      const extra = JSON.parse(String(fields.get("extra")));
      return new Response(`<input name="ipa" value="${extra.stem}">`, {
        headers: { "Content-Type": "text/html" },
      });
    }
    const asset = assets.get(url.pathname);
    if (asset)
      return new Response(asset, {
        headers: {
          "Content-Type": url.pathname.endsWith(".css")
            ? "text/css"
            : "application/javascript",
        },
      });
    const initial = (url.searchParams.get("initial") ?? "[]")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;");
    const definitions = url.searchParams.has("definitions");
    const estimator = url.searchParams.has("estimator");
    return new Response(
      `<!doctype html><html lang="en"><head>
      <meta name="viewport" content="width=device-width,initial-scale=1"><title>Extra editor regression fixture</title>
      <link rel="stylesheet" href="/main.css"><link rel="stylesheet" href="/editor.css"><link rel="stylesheet" href="/definitions.css">
      <link rel="stylesheet" href="/modal.css"><link rel="stylesheet" href="/tooltip.css">
      ${estimator ? '<script defer src="/edit.js"></script>' : ""}
      <style>body{padding:8px}form{width:100%;max-width:700px;margin:auto}*{box-sizing:border-box}</style>
      </head><body><svg style="display:none" aria-hidden="true">${icons}</svg><form id="form" class="default">${definitions ? '<div id="definitions-editor-mount"></div>' : ""}
      ${estimator ? '<label for="word">Word</label><input id="word" name="word" required value="cat"><label for="word_class">Word class</label><select id="word_class" name="word_class" required><option value="" disabled selected>-- select a word class --</option><option value="n">Noun</option></select><section><div class="ipa-estimator"><label for="ipa">IPA</label><input id="ipa" name="ipa" value="old"><button type="submit" id="estimate-ipa" formaction="/estimate-ipa">Estimate IPA</button></div></section>' : ""}
      <div id="mount"><label for="extra">Extra JSON (optional)</label><textarea id="extra" name="extra">${initial}</textarea></div>
      <button type="submit" id="submit">Save</button></form><script type="module">
      import {mountExtraEditor} from '/extra-editor.js'; mountExtraEditor('mount');
      ${definitions ? "import {mountDefinitionsEditor} from '/definitions-editor.js'; mountDefinitionsEditor('definitions-editor-mount',{initialItems:[{id:'1',definition:'original',context:''}],isEdit:true});" : ""}
      window.submissions=[];document.getElementById('form').addEventListener('submit',event=>{
        if(event.submitter?.id==='estimate-ipa')return;
        if(event.defaultPrevented)return;
        event.preventDefault();window.submissions.push([...new FormData(event.target).entries()]);
      });</script></body></html>`,
      { headers: { "Content-Type": "text/html" } },
    );
  },
});

async function desktop(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const open = async (value: unknown, definitions = false) => {
    await page.goto(
      `${server.url}?initial=${encodeURIComponent(JSON.stringify(value))}${definitions ? "&definitions" : ""}`,
    );
    await page.locator(".extra-editor").waitFor();
  };
  const data = () => page.locator("[name=extra]").inputValue().then(JSON.parse);
  const text = (name: string) =>
    page.getByRole("textbox", { name, exact: true });
  const button = (name: string) =>
    page.getByRole("button", { name, exact: true });
  const frames = () =>
    page.evaluate(
      () =>
        new Promise<void>((done) =>
          requestAnimationFrame(() =>
            requestAnimationFrame(() => requestAnimationFrame(() => done())),
          ),
        ),
    );
  const focusIs = async (locator: Locator) => {
    // Wait for logical menu closure and the actual focus contract. Firefox can
    // retain transparent transition elements after a menu has already closed.
    await expect(
      page.locator('[aria-haspopup="menu"][aria-expanded="true"]'),
    ).toHaveCount(0);
    await expect(
      locator,
      `focus target missing; data: ${await page.locator("[name=extra]").inputValue()}; errors: ${errors.join("; ")}`,
    ).toHaveCount(1);
    await expect(locator).toBeFocused();
  };
  const submissions = async () => {
    await frames();
    return page.evaluate(
      () =>
        (window as unknown as { submissions: [string, string][][] })
          .submissions,
    );
  };

  await open(["alpha", 2, false, null, []]);
  await text("Extra, item 1 (Text)").fill("new text");
  await text("Extra, item 1 (Text)").press("Enter");
  assert.deepEqual((await submissions()).at(-1), [
    ["extra", '["new text",2,false,null,[]]'],
  ]);
  await text("Extra, item 2 (Number)").fill("3.5");
  await text("Extra, item 2 (Number)").press("Enter");
  assert.deepEqual((await submissions()).at(-1), [
    ["extra", '["new text",3.5,false,null,[]]'],
  ]);
  // Restore the starting number before testing its rejected draft and movement.
  await text("Extra, item 2 (Number)").fill("2");
  await text("Extra, item 2 (Number)").press("Enter");
  await text("Extra, item 2 (Number)").fill("12oops");
  await text("Extra, item 2 (Number)").press("Enter");
  assert.equal((await submissions()).length, 3);
  assert.equal(
    await text("Extra, item 2 (Number)").getAttribute("aria-invalid"),
    "true",
  );
  const id = await text("Extra, item 2 (Number)").getAttribute("id");
  await button("Extra, item 2: move up").focus();
  await page.keyboard.press("Enter");
  assert.deepEqual(await data(), [2, "new text", false, null, []]);
  assert.equal(await page.locator(`[id="${id}"]`).inputValue(), "12oops");
  await focusIs(button("Extra, item 1: move up"));
  assert.equal(
    await button("Extra, item 1: move up").getAttribute("aria-disabled"),
    "true",
  );
  await page.keyboard.press("Enter");
  assert.deepEqual(await data(), [2, "new text", false, null, []]);
  assert.equal(await button("Raw JSON").count(), 0);
  assert.equal(await button("Structured editor").count(), 0);

  assert.equal(await page.locator("textarea").count(), 0);
  await button("Undo").click();
  await focusIs(text("Extra, item 1 (Number)"));
  await text("Extra, item 1 (Number)").fill("15");
  await button("Extra, item 1: move down").click();
  await focusIs(button("Extra, item 2: move down"));
  assert.deepEqual(await data(), ["new text", 15, false, null, []]);
  for (let position = 2; position < 5; position++) {
    await button(`Extra, item ${position}: move down`).focus();
    await page.keyboard.press("Space");
    await focusIs(button(`Extra, item ${position + 1}: move down`));
  }
  assert.deepEqual(await data(), ["new text", false, null, [], 15]);
  await page.keyboard.press("Space");
  assert.deepEqual(await data(), ["new text", false, null, [], 15]);

  // Row shortcuts follow focus, preserve drafts, and stay in the nearest list.
  await open(["first", [1, 2], { key: "value" }], true);
  await text("Extra, item 2, item 2 (Number)").focus();
  await page.keyboard.press("Control+ArrowUp");
  assert.deepEqual(await data(), ["first", [2, 1], { key: "value" }]);
  await focusIs(text("Extra, item 2, item 1 (Number)"));
  await page.keyboard.press("Control+ArrowUp");
  assert.deepEqual(await data(), ["first", [2, 1], { key: "value" }]);
  await focusIs(text("Extra, item 2, item 1 (Number)"));
  await text("Extra, item 2, item 1 (Number)").fill("unfinished");
  await page.keyboard.press("Meta+ArrowDown");
  assert.deepEqual(await data(), ["first", [1, 2], { key: "value" }]);
  await focusIs(text("Extra, item 2, item 2 (Number)"));
  assert.equal(
    await text("Extra, item 2, item 2 (Number)").inputValue(),
    "unfinished",
  );
  await text("Extra, item 2, item 2 (Number)").fill("2");
  await button("Extra, item 2: collapse collection").focus();
  await page.keyboard.press("Control+ArrowDown");
  assert.deepEqual(await data(), ["first", { key: "value" }, [1, 2]]);
  await focusIs(button("Extra, item 3: collapse collection"));
  await button("Extra, item 3: move up").focus();
  await page.keyboard.press("Control+ArrowUp");
  assert.deepEqual(await data(), ["first", [1, 2], { key: "value" }]);
  await focusIs(button("Extra, item 2: move up"));
  await button("Undo").click();
  assert.deepEqual(await data(), ["first", { key: "value" }, [1, 2]]);
  await button("Redo").click();
  assert.deepEqual(await data(), ["first", [1, 2], { key: "value" }]);
  await text('Extra, item 3, property "key": property name').focus();
  await page.keyboard.press("Control+ArrowUp");
  assert.deepEqual(await data(), ["first", [1, 2], { key: "value" }]);
  await text("Extra, item 1 (Text)").focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Control+Alt+ArrowDown");
  assert.deepEqual(await data(), ["first", [1, 2], { key: "value" }]);
  assert.equal(
    await page.getByRole("button", { name: "undo", exact: true }).isDisabled(),
    true,
  );

  await open([]);
  await button("Extra: add at beginning").focus();
  await page.keyboard.press("Enter");
  await page.getByRole("menu").waitFor();
  await page.keyboard.press("Escape");
  await focusIs(button("Extra: add at beginning"));
  for (const type of ["Text", "Number", "Boolean", "List", "Object", "Null"]) {
    await open([]);
    await button("Extra: add at beginning").focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Home");
    const offset = [
      "Text",
      "Number",
      "Boolean",
      "Object",
      "List",
      "Null",
    ].indexOf(type);
    for (let i = 0; i < offset; i++) await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    const target =
      type === "Text" || type === "Number"
        ? text(`Extra, item 1 (${type})`)
        : type === "Boolean"
          ? page.getByRole("checkbox", {
              name: "Extra, item 1",
              exact: true,
            })
          : button(
              type === "Null"
                ? "Extra, item 1: remove"
                : "Extra, item 1: add at end",
            );
    await focusIs(target);
  }

  await open({ a: 1, b: 2 });
  await text('Extra, property "a": property name').fill("b");
  await text('Extra, property "a": property name').press("Enter");
  assert.equal((await submissions()).length, 0);
  await button("Undo").click();
  await focusIs(text('Extra, property "a": property name'));
  await button('Extra, property "b": remove').click();
  await frames();
  assert.equal(
    await text('Extra, property "a": property name').getAttribute(
      "aria-invalid",
    ),
    "false",
  );
  await button("Save").click();
  assert.deepEqual((await submissions()).at(-1), [["extra", '{"b":1}']]);
  assert.deepEqual(await data(), { b: 1 });

  await open(JSON.parse('{"__proto__":false,"":[]}'));
  assert.deepEqual(await data(), JSON.parse('{"__proto__":false,"":[]}'));
  assert.equal(await page.locator("[name=extra]").count(), 1);
  await page
    .getByRole("checkbox", {
      name: 'Extra, property "__proto__"',
      exact: true,
    })
    .check();
  assert.deepEqual(await data(), JSON.parse('{"__proto__":true,"":[]}'));

  // Only an empty object skips confirmation; empty lists and falsy values count.
  const clearDialog = page.getByRole("dialog", {
    name: "Clear all extra data?",
  });
  for (const value of [{ nested: { items: [1] } }, [], "", 0, false, null]) {
    await open(value);
    await button("Remove extra data").click();
    await clearDialog.waitFor({ state: "attached" });
    await clearDialog.locator(".modal-panel").waitFor();
    await focusIs(clearDialog.getByRole("button", { name: "Cancel" }));
    assert.deepEqual(await data(), value);
    await page.keyboard.press("Control+z");
    assert.deepEqual(await data(), value);
    await clearDialog.getByRole("button", { name: "Cancel" }).click();
    await clearDialog.waitFor({ state: "detached" });
    await focusIs(button("Remove extra data"));
    assert.deepEqual(await data(), value);
    assert.equal(await button("Undo").isDisabled(), true);
    await button("Remove extra data").click();
    // Wait for the dialog's focus trap before sending Escape. The click can
    // finish before Headless UI has installed its keyboard handlers.
    await expect(
      clearDialog.getByRole("button", { name: "Cancel" }),
    ).toBeFocused();
    await expect(clearDialog.locator(".modal-panel")).toHaveCSS("opacity", "1");
    await page.keyboard.press("Escape");
    await clearDialog.waitFor({ state: "detached" });
    assert.deepEqual(await data(), value);
    await button("Remove extra data").click();
    await clearDialog.getByRole("button", { name: "Clear all data" }).click();
    await clearDialog.waitFor({ state: "detached" });
    await focusIs(button("Add extra data"));
    assert.equal(await page.locator("[name=extra]").inputValue(), "");
    await page.keyboard.press("Control+z");
    assert.deepEqual(await data(), value);
  }

  // Malformed nonempty starting data gets a raw editor. Repairing it
  // keeps that editor usable; clearing returns to the normal empty state.
  await page.goto(
    `${server.url}?initial=${encodeURIComponent("broken JSON  ")}`,
  );
  await text("Extra JSON (optional)").waitFor();
  assert.equal(
    await text("Extra JSON (optional)").inputValue(),
    "broken JSON  ",
  );
  assert.equal(
    await text("Extra JSON (optional)").getAttribute("aria-invalid"),
    "true",
  );
  assert.equal(await button("Structured editor").count(), 0);
  await button("Remove extra data").click();
  await clearDialog.waitFor({ state: "attached" });
  await clearDialog.locator(".modal-panel").waitFor();
  await clearDialog.getByRole("button", { name: "Cancel" }).click();
  await clearDialog.waitFor({ state: "detached" });
  assert.equal(
    await text("Extra JSON (optional)").inputValue(),
    "broken JSON  ",
  );
  await button("Save").click();
  assert.equal((await submissions()).length, 0);
  await text("Extra JSON (optional)").fill("1e999");
  await button("Save").click();
  assert.equal((await submissions()).length, 0);
  await text("Extra JSON (optional)").fill('{"fixed":true}');
  await frames();
  assert.equal(
    await text("Extra JSON (optional)").evaluate(
      (element) => (element as HTMLTextAreaElement).validationMessage,
    ),
    "",
  );
  // Firefox's native validation popup can consume a subsequent pointer click.
  await button("Save").press("Enter");
  assert.deepEqual((await submissions()).at(-1), [["extra", '{"fixed":true}']]);
  await button("Undo").click();
  assert.equal(await text("Extra JSON (optional)").inputValue(), "1e999");
  await button("Redo").click();
  await text("Extra JSON (optional)").fill("");
  await focusIs(button("Add extra data"));
  assert.equal(await page.locator("textarea").count(), 0);
  await button("Save").click();
  assert.deepEqual((await submissions()).at(-1), [["extra", ""]]);
  await button("Add extra data").focus();
  await page.keyboard.press("Control+z");
  assert.equal(
    await text("Extra JSON (optional)").inputValue(),
    '{"fixed":true}',
  );
  await button("Remove extra data").click();
  await clearDialog.getByRole("button", { name: "Clear all data" }).click();
  await clearDialog.waitFor({ state: "detached" });
  await button("Add extra data").click();
  assert.deepEqual(await data(), {});
  assert.equal(await page.locator("textarea").count(), 0);

  // A raw editor repaired to an empty object also clears without confirmation.
  await page.goto(`${server.url}?initial=broken`);
  await text("Extra JSON (optional)").fill(" { } ");
  await button("Remove extra data").click();
  await focusIs(button("Add extra data"));
  assert.equal(await clearDialog.count(), 0);

  // Blank and whitespace-only values start collapsed and submit a blank.
  for (const initial of ["", " \n\t"]) {
    await page.goto(`${server.url}?initial=${encodeURIComponent(initial)}`);
    await button("Add extra data").waitFor();
    assert.equal(await page.locator(".extra-editor-heading").count(), 0);
    assert.equal(await page.locator("textarea").count(), 0);
    assert.equal(await page.locator("[name=extra]").count(), 1);
    if (process.env.EXTRA_EDITOR_SCREENSHOTS)
      await page.screenshot({
        path: `${process.env.EXTRA_EDITOR_SCREENSHOTS}/empty.png`,
        fullPage: true,
      });
    await button("Save").click();
    assert.deepEqual((await submissions()).at(-1), [["extra", ""]]);
    await button("Add extra data").click();
    await focusIs(button("Extra: add at end"));
    assert.deepEqual(await data(), {});
    await page.keyboard.press("Control+z");
    await focusIs(button("Add extra data"));
    assert.equal(await page.locator("[name=extra]").inputValue(), "");
    await page.keyboard.press("Control+Shift+z");
    assert.deepEqual(await data(), {});
    await button("Remove extra data").click();
    await focusIs(button("Add extra data"));
    assert.equal(await clearDialog.count(), 0);
    await page.keyboard.press("Control+z");
    assert.deepEqual(await data(), {});
    await page.keyboard.press("Control+y");
    await focusIs(button("Add extra data"));
    assert.equal(await page.locator("[name=extra]").inputValue(), "");
  }

  // Clearing invalid drafts restores saved values, with all earlier history.
  await open({ count: 1 });
  await text('Extra, property "count" (Number)').fill("2");
  await text('Extra, property "count" (Number)').press("Tab");
  await text('Extra, property "count" (Number)').fill("invalid");
  await button("Remove extra data").click();
  await clearDialog.getByRole("button", { name: "Cancel" }).click();
  await clearDialog.waitFor({ state: "detached" });
  assert.equal(
    await text('Extra, property "count" (Number)').inputValue(),
    "invalid",
  );
  assert.deepEqual(await data(), { count: 2 });
  await button("Remove extra data").click();
  await clearDialog.getByRole("button", { name: "Clear all data" }).click();
  await clearDialog.waitFor({ state: "detached" });
  await focusIs(button("Add extra data"));
  assert.equal(await page.locator(".extra-editor-heading").count(), 0);
  await button("Save").click();
  assert.deepEqual((await submissions()).at(-1), [["extra", ""]]);
  await button("Add extra data").focus();
  await page.keyboard.press("Control+z");
  assert.deepEqual(await data(), { count: 2 });
  assert.equal(
    await text('Extra, property "count" (Number)').inputValue(),
    "2",
  );
  await button("Undo").click();
  assert.deepEqual(await data(), { count: 1 });
  await button("Redo").click();
  assert.deepEqual(await data(), { count: 2 });
  await button("Redo").click();
  await focusIs(button("Add extra data"));
  await button("Add extra data").click();
  assert.deepEqual(await data(), {});
  await button("Undo").click();
  await focusIs(button("Add extra data"));
  await page.keyboard.press("Control+z");
  assert.deepEqual(await data(), { count: 2 });
  assert.equal(await button("Redo").isEnabled(), true);
  await button("Extra: add at beginning").click();
  await page.getByRole("menuitem", { name: "Boolean", exact: true }).click();
  assert.equal(await button("Redo").isDisabled(), true);

  await button("Extra editor help").click();
  const help = page.getByRole("dialog", { name: "Extra editor help" });
  await help.waitFor({ state: "attached" });
  await help.locator(".modal-panel").waitFor();
  await page.waitForFunction(() => {
    const panel = document.querySelector(".modal-panel");
    return panel && getComputedStyle(panel).opacity === "1";
  });
  assert.equal(
    await help
      .locator("kbd")
      .allTextContents()
      .then((keys) => keys.join(" ")),
    "Ctrl ↑ Ctrl ↓ Ctrl z Ctrl Shift z Ctrl y",
  );
  if (process.env.EXTRA_EDITOR_SCREENSHOTS)
    await page.screenshot({
      path: `${process.env.EXTRA_EDITOR_SCREENSHOTS}/help.png`,
      fullPage: true,
    });
  await page.keyboard.press("Escape");
  await help.waitFor({ state: "detached" });
  await focusIs(button("Extra editor help"));
  if (process.env.EXTRA_EDITOR_SCREENSHOTS)
    await page.screenshot({
      path: `${process.env.EXTRA_EDITOR_SCREENSHOTS}/opened.png`,
      fullPage: true,
    });

  await open([1]);
  await text("Extra, item 1 (Number)").fill("1.0");
  await text("Extra, item 1 (Number)").press("Tab");
  assert.equal(await text("Extra, item 1 (Number)").inputValue(), "1");
  assert.equal(await button("Undo").isDisabled(), true);

  // All JSON root types are supported, not only collections.
  for (const primitive of ["root", 42, false, null]) {
    await open(primitive);
    assert.deepEqual(await data(), primitive);
    assert.equal(await page.locator("[name=extra]").count(), 1);
    await button("Remove extra data").click();
    await clearDialog.getByRole("button", { name: "Clear all data" }).click();
    await clearDialog.waitFor({ state: "detached" });
    await focusIs(button("Add extra data"));
    await page.keyboard.press("Control+z");
    await focusIs(button("Remove extra data"));
    assert.deepEqual(await data(), primitive);
  }

  await open({ a: 1, key: 2 });
  await text('Extra, property "a": property name').fill("key2");
  await button("Extra: add at beginning").click();
  await page.getByRole("menuitem", { name: "Text", exact: true }).click();
  assert.deepEqual(await data(), { key3: "", key2: 1, key: 2 });
  await focusIs(text('Extra, property "key3": property name'));

  await open({ nested: [1, 2], keep: "yes" });
  await text('Extra, property "nested", item 1 (Number)').fill("bad");
  await button('Extra, property "nested": remove').click();
  await focusIs(text('Extra, property "keep": property name'));
  assert.deepEqual(await data(), { keep: "yes" });
  await button("Undo").click();
  assert.deepEqual(await data(), { nested: [1, 2], keep: "yes" });
  assert.equal(
    await text('Extra, property "nested", item 1 (Number)').inputValue(),
    "1",
  );
  await text('Extra, property "keep" (Text)').focus();
  await button("Redo").click();
  assert.deepEqual(await data(), { keep: "yes" });
  await button('Extra, property "keep": remove').click();
  await focusIs(button("Extra: add at end"));

  await open([""]);
  await text("Extra, item 1 (Text)").fill("pending");
  assert.equal(await button("Undo").isEnabled(), true);
  await text("Extra, item 1 (Text)").press("Tab");
  await button("Undo").click();
  assert.deepEqual(await data(), [""]);
  await text("Extra, item 1 (Text)").focus();
  await text("Extra, item 1 (Text)").press("Tab");
  assert.equal(await button("Redo").isEnabled(), true);
  await button("Redo").click();
  assert.deepEqual(await data(), ["pending"]);

  await open(["original"], true);
  await page.locator('input[name="definitions[]"]').waitFor();
  const styles = await page
    .locator('[aria-label="undo"], [aria-label="Undo"]')
    .evaluateAll((buttons) =>
      buttons.map((button) => {
        const style = getComputedStyle(button);
        return [
          style.backgroundColor,
          style.borderColor,
          style.color,
          style.padding,
        ];
      }),
    );
  assert.deepEqual(styles[0], styles[1]);
  await text("Extra, item 1 (Text)").focus();
  await page.keyboard.press("Control+Enter");
  assert.equal(await page.locator('input[name="definitions[]"]').count(), 1);
  assert.equal(
    await page
      .getByRole("button", { name: "undo", exact: true })
      .getAttribute("disabled"),
    "",
  );
  await text("Extra, item 1 (Text)").fill("edited");
  // The definitions editor must not prevent native shortcuts in Extra.
  assert.equal(
    await text("Extra, item 1 (Text)").evaluate((input) =>
      input.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "z",
          ctrlKey: true,
          bubbles: true,
          cancelable: true,
        }),
      ),
    ),
    true,
  );
  await text("Extra, item 1 (Text)").press("Tab");
  await button("Extra, item 1: remove").focus();
  await page.keyboard.press("Control+z");
  assert.deepEqual(await data(), ["original"]);
  assert.equal(
    await page.getByRole("button", { name: "undo", exact: true }).isDisabled(),
    true,
  );
  await page.locator('input[name="definitions[]"]').fill("changed definition");
  await page.locator('input[name="definitions[]"]').press("Tab");
  await page.getByRole("button", { name: "undo", exact: true }).focus();
  await page.keyboard.press("Control+z");
  await frames();
  assert.equal(
    await page.locator('input[name="definitions[]"]').inputValue(),
    "original",
  );
  assert.deepEqual(await data(), ["original"]);
  // Key examples show the platform's modifier without a separate Mac note.
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "platform", { value: "MacIntel" });
  });
  await open([], true);
  await button("add definition").focus();
  await page.keyboard.press("Meta+Enter");
  assert.equal(await page.locator('input[name="definitions[]"]').count(), 2);
  await button("Extra editor help").click();
  const macHelp = page.getByRole("dialog", { name: "Extra editor help" });
  await macHelp.waitFor({ state: "attached" });
  await macHelp.locator(".modal-panel").waitFor();
  assert.equal(
    await macHelp
      .locator("kbd")
      .allTextContents()
      .then((keys) => keys.join(" ")),
    "Command ↑ Command ↓ Command z Command Shift z Command y",
  );
  assert.equal((await macHelp.textContent())?.includes("On Mac"), false);
  assert.deepEqual(errors, []);
}

async function lossless(page: Page) {
  for (const raw of [
    '{"s":"line1\\nline2"}',
    '{"a\\nb":"value"}',
    '{"s":"line1\\r\\nline2"}',
    '{"a\\rb":"value"}',
    '{"n":9007199254740993}',
    '{"nested":[18446744073709551615,-9007199254740993]}',
    '{"n":0.10000000000000001}',
    '{"n":1e-999}',
  ]) {
    await page.goto(`${server.url}?initial=${encodeURIComponent(raw)}`);
    const field = page.getByRole("textbox", {
      name: "Extra JSON (optional)",
      exact: true,
    });
    await expect(field).toHaveValue(raw);
    await expect(field).toHaveAttribute("aria-invalid", "false");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    assert.deepEqual(
      await page.evaluate(
        () =>
          (window as unknown as { submissions: [string, string][][] })
            .submissions,
      ),
      [[["extra", raw]]],
      "saving untouched JSON must preserve strings, keys, and numbers",
    );
    const edited = raw.replace(/}$/, ',"added":true}');
    await field.fill(edited);
    await page.getByRole("button", { name: "Save", exact: true }).click();
    assert.equal(await page.locator("[name=extra]").inputValue(), edited);
  }

  await page.goto(`${server.url}?initial=${encodeURIComponent("[1]")}`);
  const number = page.getByRole("textbox", {
    name: "Extra, item 1 (Number)",
    exact: true,
  });
  await number.fill("9007199254740993");
  await number.press("Enter");
  await expect(number).toHaveAttribute("aria-invalid", "true");
  assert.equal(await page.locator("[name=extra]").inputValue(), "[1]");
  assert.deepEqual(
    await page.evaluate(
      () =>
        (window as unknown as { submissions: [string, string][][] })
          .submissions,
    ),
    [],
  );
  await number.fill("1.0");
  await number.press("Enter");
  await expect(number).toHaveAttribute("aria-invalid", "false");
  assert.deepEqual(
    await page.evaluate(
      () =>
        (window as unknown as { submissions: [string, string][][] })
          .submissions,
    ),
    [[["extra", "[1]"]]],
  );
}

async function folding(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const button = (name: string) =>
    page.getByRole("button", { name, exact: true });
  const text = (name: string) =>
    page.getByRole("textbox", { name, exact: true });
  const data = () => page.locator("[name=extra]").inputValue().then(JSON.parse);
  const initial = { nested: [1, 2], keep: "yes" };
  await page.goto(
    `${server.url}?initial=${encodeURIComponent(JSON.stringify(initial))}`,
  );
  const summary = button('Extra, property "nested": collapse collection');
  await expect(summary).toBeVisible();
  const collectionId = await summary.getAttribute("aria-controls");
  assert(collectionId);
  const collection = page.locator(`[id="${collectionId}"]`);
  const input = text('Extra, property "nested", item 1 (Number)');

  // The compact fold control and collection summary control the same group.
  await expect(button('Extra, property "nested": collapse')).toHaveAttribute(
    "aria-controls",
    collectionId,
  );
  await summary.click();
  await expect(collection).toBeHidden();
  await expect(
    button('Extra, property "nested": expand collection'),
  ).toHaveAttribute("aria-expanded", "false");
  await expect(button("Undo")).toBeDisabled();
  assert.deepEqual(await data(), initial);
  await button('Extra, property "nested": expand').click();
  await expect(collection).toBeVisible();

  // Invalid descendants stay visible and focused, including when folding an
  // ancestor; valid drafts commit on blur and survive a fold/expand cycle.
  await input.fill("unfinished");
  for (const control of [
    'Extra, property "nested": collapse collection',
    'Extra, property "nested": collapse',
    "Extra: collapse",
  ]) {
    await button(control).click();
    await expect(input).toBeFocused();
    await expect(collection).toBeVisible();
    await expect(input).toHaveValue("unfinished");
    assert.deepEqual(await data(), initial);
  }
  await input.fill("3");
  await button('Extra, property "nested": collapse').click();
  await expect(collection).toBeHidden();
  assert.deepEqual(await data(), { nested: [3, 2], keep: "yes" });

  // Adding from the row controls expands a folded collection and focuses the
  // inserted value. Folding itself must not add a JSON history entry.
  await button('Extra, property "nested": add at beginning').click();
  await page.getByRole("menuitem", { name: "Number", exact: true }).click();
  await expect(collection).toBeVisible();
  await expect(text('Extra, property "nested", item 1 (Number)')).toBeFocused();
  assert.deepEqual(await data(), { nested: [0, 3, 2], keep: "yes" });
  await button("Undo").click();
  assert.deepEqual(await data(), { nested: [3, 2], keep: "yes" });
  await button("Undo").click();
  assert.deepEqual(await data(), initial);
  await expect(button("Undo")).toBeDisabled();

  // Root folding preserves serialization and native submission. The heading
  // add control remains usable while the root's contents are hidden.
  await button("Extra: collapse").click();
  await expect(text('Extra, property "keep" (Text)')).toBeHidden();
  await button("Save").click();
  assert.deepEqual(
    await page.evaluate(() =>
      (
        window as unknown as { submissions: [string, string][][] }
      ).submissions.at(-1),
    ),
    [["extra", JSON.stringify(initial)]],
  );
  await button("Extra: add at beginning").click();
  await page.getByRole("menuitem", { name: "Null", exact: true }).click();
  await expect(text('Extra, property "key": property name')).toBeFocused();
  assert.deepEqual(await data(), { key: null, ...initial });
  assert.deepEqual(errors, []);
}

async function touch(page: Page) {
  const value = { ["long property ".repeat(12)]: { "": [[[[[1, 2, 3]]]]] } };
  await page.goto(
    `${server.url}?initial=${encodeURIComponent(JSON.stringify(value))}`,
  );
  await page.locator(".extra-editor").waitFor();
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
    true,
    "deep collections must fit at 320px",
  );
  const list = page.locator(".extra-editor-collection").last();
  const firstUp = list.getByRole("button", { name: /item 1: move up$/ });
  const firstDown = list.getByRole("button", { name: /item 1: move down$/ });
  assert.equal(await firstUp.getAttribute("aria-disabled"), "true");
  await firstDown.tap();
  const values = await list
    .locator('input[type="text"]')
    .evaluateAll((inputs) =>
      inputs.map((input) => (input as HTMLInputElement).value),
    );
  assert.deepEqual(values, ["2", "1", "3"]);
  const add = list.getByRole("button", { name: /: add at end$/ });
  const box = await add.boundingBox();
  assert(box && box.width >= 44 && box.height >= 44);
  await add.tap();
  const menu = page.getByRole("menu");
  await menu.waitFor();
  const menuBox = await menu.boundingBox();
  assert(menuBox && menuBox.x >= 0 && menuBox.x + menuBox.width <= 320);
  await page.getByRole("menuitem", { name: "Null", exact: true }).tap();
  assert.equal(
    await list.getByRole("button", { name: /item 4: remove$/ }).count(),
    1,
  );
  assert.equal(await page.locator("[name=extra]").count(), 1);
  const serialized = await page.locator("[name=extra]").inputValue();
  assert(serialized.includes("[2,1,3,null]"));

  // Narrow layouts omit the add-at-beginning controls; the collection summary
  // and footer still let touch users fold, expand, and add values.
  assert(
    await page
      .locator(".extra-editor-add-start")
      .evaluateAll((buttons) =>
        buttons.every((button) => getComputedStyle(button).display === "none"),
      ),
  );
  await page
    .getByRole("button", { name: "Extra: collapse collection", exact: true })
    .tap();
  await expect(list).toBeHidden();
  assert.equal(await page.locator("[name=extra]").inputValue(), serialized);
  await page
    .getByRole("button", { name: "Extra: expand collection", exact: true })
    .tap();
  await expect(list).toBeVisible();
  assert.equal(await page.locator("[name=extra]").inputValue(), serialized);
}

async function estimator(page: Page) {
  const requests: URLSearchParams[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/estimate-ipa")
      requests.push(new URLSearchParams(request.postData() ?? ""));
  });
  await page.goto(
    `${server.url}?estimator&initial=${encodeURIComponent('{"stem":"c","count":1}')}`,
  );
  const stem = page.getByRole("textbox", {
    name: 'Extra, property "stem" (Text)',
    exact: true,
  });
  const count = page.getByRole("textbox", {
    name: 'Extra, property "count" (Number)',
    exact: true,
  });
  const ipa = page.locator("#ipa");
  const estimate = page.locator("#estimate-ipa");
  // A fresh new-word form omits the disabled word-class placeholder. Automatic
  // estimation must work with that payload while Save still requires a class.
  await page.locator("#word").fill("ca");
  await page.locator("#word").fill("cat");
  await expect(ipa).toHaveValue("c");
  await expect(estimate).toBeEnabled();
  assert.equal(requests.length, 1);
  assert.equal(requests[0]!.has("word_class"), false);
  assert.equal(await page.locator(".field-errors").count(), 0);
  await page.getByRole("button", { name: "Save", exact: true }).click();
  assert.deepEqual(
    await page.evaluate(
      () =>
        (window as unknown as { submissions: [string, string][][] }).submissions,
    ),
    [],
    "Save must require a word class even though automatic estimation does not",
  );
  await page.locator("#word_class").selectOption("n");
  requests.length = 0;
  await stem.fill("a");
  await count.fill("2");
  // Implicit submission must flush a draft even when it hasn't blurred.
  await count.press("Enter");
  await expect(ipa).toHaveValue("a");
  await expect(estimate).toBeEnabled();
  assert.equal(requests.length, 1);
  assert.deepEqual(JSON.parse(requests[0]!.get("extra")!), {
    stem: "a",
    count: 2,
  });
  assert.equal(requests[0]!.get("ipa"), "c");
  assert.equal(requests[0]!.get("word"), "cat");
  assert.equal(requests[0]!.get("word_class"), "n");

  await count.fill("unfinished");
  await count.press("Enter");
  await expect(count).toHaveAttribute("aria-invalid", "true");
  assert.equal(requests.length, 1);
  await page
    .locator("#word")
    .fill("invalid extra must block automatic estimation");
  // Wait beyond the debounce delay to check that no automatic request occurs.
  await page.evaluate(
    () => new Promise<void>((resolve) => setTimeout(resolve, 600)),
  );
  assert.equal(requests.length, 1);
  await count.fill("3");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  assert.equal(
    requests.length,
    1,
    "Save must remain an ordinary form submission",
  );
  const submissions = await page.evaluate(
    () =>
      (window as unknown as { submissions: [string, string][][] }).submissions,
  );
  assert.deepEqual(JSON.parse(new Map(submissions.at(-1)).get("extra")!), {
    stem: "a",
    count: 3,
  });

  // Automatic estimation uses the same form payload and one debounce timer.
  await page.locator("#word").fill("c");
  await page.locator("#word").fill("ca");
  await page.locator("#word").fill("cat");
  await expect.poll(() => requests.length).toBe(2);
  await expect(estimate).toBeEnabled();
  assert.equal(requests[1]!.get("word"), "cat");
  assert.deepEqual(JSON.parse(requests[1]!.get("extra")!), {
    stem: "a",
    count: 3,
  });

  await page.route("**/estimate-ipa", (route) =>
    route.fulfill({
      status: 400,
      contentType: "text/html",
      body: '<p class="error">Extra must be valid JSON</p><input name="ipa" value="wrong">',
    }),
  );
  await estimate.click();
  await expect(page.locator(".field-errors")).toHaveText(
    "Extra must be valid JSON",
  );
  await expect(ipa).toHaveValue("a");
  await expect(estimate).toBeEnabled();
}

async function scenario(
  name: string,
  page: Page,
  run: (page: Page) => Promise<void>,
) {
  const context = page.context();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await context.tracing.start({
    screenshots: true,
    snapshots: true,
    sources: true,
  });
  let artifacts: string | undefined;
  try {
    await run(page);
    assert.deepEqual(errors, [], `${name}: uncaught browser errors`);
  } catch (error) {
    artifacts = resolve(
      process.env.EXTRA_EDITOR_ARTIFACTS ??
        resolve(frontend, "test-results/extra-editor"),
      name,
    );
    await mkdir(artifacts, { recursive: true });
    const captures = await Promise.allSettled([
      page.screenshot({
        path: resolve(artifacts, "failure.png"),
        fullPage: true,
        timeout: 5000,
      }),
      Bun.write(
        resolve(artifacts, "failure.json"),
        JSON.stringify(
          { scenario: name, url: page.url(), errors, error: String(error) },
          null,
          2,
        ),
      ),
    ]);
    for (const capture of captures)
      if (capture.status === "rejected")
        console.error(`${name}: could not capture failure: ${capture.reason}`);
    console.error(`${name} failed; artifacts: ${artifacts}`);
    throw error;
  } finally {
    await context.tracing
      .stop(artifacts ? { path: resolve(artifacts, "trace.zip") } : {})
      .catch((error) =>
        console.error(`${name}: could not save trace: ${error}`),
      );
    await page.close();
  }
}

try {
  for (const name of engines) {
    const browser = await (name === "chromium" ? chromium : firefox).launch({
      executablePath:
        process.env[`EXTRA_EDITOR_${name.toUpperCase()}_EXECUTABLE`],
    });
    try {
      await scenario(
        `${name}-desktop`,
        await browser.newPage({
          viewport: { width: 900, height: 800 },
        }),
        desktop,
      );
      console.log(`${name}: keyboard, form, and history checks passed`);
      await scenario(`${name}-lossless`, await browser.newPage(), lossless);
      console.log(`${name}: lossless JSON checks passed`);
      await scenario(
        `${name}-folding`,
        await browser.newPage({
          viewport: { width: 900, height: 800 },
        }),
        folding,
      );
      console.log(`${name}: collection folding checks passed`);
      await scenario(`${name}-estimator`, await browser.newPage(), estimator);
      console.log(`${name}: IPA estimator form checks passed`);
      if (name === "chromium") {
        const mobile = await browser.newPage({
          viewport: { width: 320, height: 740 },
          isMobile: true,
          hasTouch: true,
        });
        await scenario(`${name}-touch`, mobile, touch);
      }
      console.log(
        `${name}: extra editor keyboard, form, history${name === "chromium" ? ", and 320px touch" : ""} checks passed`,
      );
    } finally {
      await browser.close();
    }
  }
} finally {
  server.stop(true);
  clearTimeout(timeout);
}
