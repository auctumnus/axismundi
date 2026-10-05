# Structured extra editor: collection editing plan

## Goal

Build a recursive CollectionEditor alongside PrimitiveEditor, with valid JSON
updates, typed insertion menus, and list reordering usable by touch, keyboard,
and assistive technology. No dedicated node-type changing control.

## Reviewed design

1. **One tree owns the data.** CollectionEditor receives `tree`, a collection
   node, and a descriptive label. An editor context provides update/draft/focus
   services. Children dispatch
   `JsonUpdate` actions instead of reconstructing a whole object/array from
   captured props. React keys are stable value-node IDs, never array indices or
   object names. Repair the missing parent insertion in `attach` first.
2. **Recursive form controls.** Render collections as labeled groups containing
   ordered lists (arrays) or unordered lists (objects). Each entry has its
   current position/key, a recursive value editor, and Remove. Object names are
   editable string drafts; duplicate names produce an inline error and preserve
   the committed key. Empty and prototype-like names remain legal.
3. **Typed additions.** Put a small + menu before the first entry and after
   each entry, including empty collections. Use the installed Headless UI Menu
   for keyboard navigation, Escape/outside-click dismissal, and focus return.
   Menu actions insert defaults: text `""`, number `0`, boolean `false`, null
   `null`, object `{}`, list `[]`. Object additions generate an unused `key`,
   `key2`, etc., then focus the name for editing. Array additions focus their
   first editor control. Type labels distinguish Null from List.
4. **Array movement.** Always-visible, comfortably sized Up/Down buttons on
   each array entry; first/last controls are unavailable. Each moves one slot
   using the existing post-detach destination-index convention. Keyboard users
   Tab to controls and press Enter/Space. Repeated moves preserve the same
   node/draft and keep focus on its controls, including at a boundary. No drag
   dependency or global shortcuts; ordinary scrolling and text editing work.
5. **Focus.** After insertion focus the new entry; after
   removal focus the next entry, previous entry, or the remaining add button.
   Array labels
   are 1-based and update with position. All controls have contextual names;
   errors link to their inputs. Avoid listbox/tree/toolbar roles that would
   require a separate composite-widget keyboard model.
6. **Drafts and atomic commits.** Keep primitive/key drafts local and commit on
   blur. Structural updates use the latest tree even if blur commits immediately
   precede a click. Reject failed updates without mutating data or losing drafts.
   Invalid drafts block form submission with native custom validity. A small
   editor Undo/Redo group allows recovery from accidental subtree removal.
7. **Usable integration.** Replace the empty ExtraEditor with a tree owner and
   recursive root editor, retain exactly one successful `name=extra` form
   control. Blank data starts collapsed. Invalid JSON, strings/keys containing
   line breaks, and numbers that would round during serialization use raw mode.
   Other valid JSON starts structured. Raw edits preserve the submitted text;
   serialization follows every committed structured update. Preserve the host
   textarea's accessible label and avoid remounting the host form.
8. **Responsive presentation.** Reuse normal input/button styling and the
   site's color variables. Wrap row actions, give fields min-width zero, use
   modest nested indentation, keep menus within the viewport. Test at 320px.

## Validation

- Run the existing tree tests first; add regressions for UI-driven insertion,
  adjacent/boundary moves, nested edits, stable IDs, and atomic failure.
- Typecheck and build the frontend.
- Exercise the rendered editor in a real browser: keyboard-only insertion/menu
  cancellation, repeated reordering to either boundary, deleting last/nested
  entries, duplicate keys, invalid primitive drafts, blur followed by a
  structural action, form serialization and mode changes, undo/redo.
- Repeat touch insertion/movement on a narrow viewport and inspect wrapping,
  menu placement, names, and focus. Browser checks cannot
  establish full screen-reader support; report that limit candidly.

## Reference basis

- Existing definitions editor: explicit move actions, stable keys, local drafts,
  undo snapshots. Existing phonology editor: reducer-owned data, explicit focus.
- [WAI listbox pattern](https://www.w3.org/WAI/ARIA/apg/patterns/listbox/):
  options are unsuitable for nested interactive form controls.
- [WCAG dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html):
  visible movement buttons provide a single-pointer operation.
- [Headless UI Menu](https://headlessui.com/react/menu) and
  [WAI menu button pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/):
  use the existing library for menu behavior instead of a custom keyboard model.

## Adversarial review

### Round 1 decisions

Independent accessibility and data reviewers both identified submit-without-blur,
history/mode draft loss, and focus restoration as blockers. Resolve as follows:

- A draft registry exposes validation/flush functions for current primitive and
  key drafts. Set native custom validity as drafts change, not just on blur.
  Capture host-form submission to flush valid drafts and synchronously write the
  successful `extra` control before other submission listeners read FormData.
  Also validate/flush before raw/structured switches and Undo/Redo; invalid
  drafts abort the action and focus the first invalid input. Removing an
  entry explicitly discards its subtree drafts; Undo restores committed data.
- The owner reads and writes an authoritative tree/state ref synchronously,
  with React state used for rendering. No updater callbacks with side effects.
  This serializes a blur followed by a structural click in the same event turn.
- Undo/Redo deliberately resets draft editors using a revision key; reordering
  does not. A history action after flushing a dirty valid draft undoes that
  newly committed edit first. Snapshot history is bounded.
- Use focusable `aria-disabled` boundary movement buttons with no-op guards.
  Explicitly restore the exact moved button after the DOM move. Menu actions
  close before scheduling stable-ID insertion focus after the next render;
  menu cancellation leaves library focus restoration intact.
- Give every collection an actual visible labeled group and every primitive/key
  input its full contextual name (including parent and 1-based array position).
  A visible host label names the structured root group; in raw mode it labels
  the textarea. It never targets the hidden successful form input.
- Boolean uses a labeled checkbox, Null a noneditable labeled `null` value.
- Mode conversion rejects nonfinite numbers such as `1e999`, despite
  JSON.parse accepting them as Infinity. Raw invalid text remains lossless;
  raw form validation permits blank but rejects invalid/nonfinite JSON.

### Round 2 refinements

- Commit callbacks return the reducer's synchronous result. Failed actions add
  no history. Unchanged primitive/key commits are
  no-ops and preserve Redo.
- Resolve insertion positions using stable entry anchors against the latest
  collection, and derive newly inserted IDs from the returned tree. Never use
  a menu-open-time index or key availability.
- Draft registration cleanup checks identity; flush order follows document
  order, not registry insertion order. Revalidate duplicate drafts when sibling
  keys change or disappear. Error messages appear inline.
- Focus requests have a cancellation token and verify the target still exists.
  Null insertion focuses its Remove button; new collections their internal +
  button. Test final focus after the menu's two-frame restoration completes.
- Blank keys receive an audible label without rewriting data. Nested controls
  include parent context. Reduce indentation at narrow widths, including deep
  nesting; avoid accumulating fixed padding. All action buttons are type=button.
- Raw mode permits empty text to clear optional data, validates all nonblank
  input, and keeps exact text. There is no manual raw-to-structured switch.

Both reviewers found no remaining design blocker in round 2. Their subsequent
implementation review found no collection/data blocker, and identified two
integration corrections that were implemented: history availability must
account for dirty drafts; definitions-editor shortcuts must stay inside their
mount instead of intercepting native text shortcuts in Extra. Raw validation
also checks parsed numbers without generating disposable UUID-backed trees.

## Implementation and reproducible checks

- `extra-editor.tsx`: recursive collection/value controls, typed insertion
  menus, editable property names, guarded list movement, draft registry,
  synchronous serialization, focus restoration, raw mode, and 100-snapshot
  Undo/Redo. Removal intentionally discards invalid drafts in that subtree;
  history restores committed values. Raw edits also support Undo/Redo.
- `extra-editor-tree.ts`: restored the missing `attach` splice. All 149
  existing frontend tests pass, including the insert/move regression cases.
- `extra-editor.css`: wrapping controls, 44px movement/menu targets, viewport
  menu constraints, and capped nesting indentation.
- `tests/extra-editor.browser.ts`: public-mount fixture using actual site CSS
  and native forms; keyboard insertion for every type, focus after menu close,
  boundary moves, invalid drafts, submit without blur, duplicate-key recovery,
  subtree removal/history, generated key collisions, optional blank raw data,
  and isolation from definitions-editor shortcuts. Includes 320px touch
  movement/insertion and deeply nested long/empty keys.

From `frontend`, install browser binaries once with
`bun run test:extra-editor:install` (add `--with-deps` on supported Linux
systems to install system libraries), then run:

```sh
bun run typecheck
bun test
bun run build
bun run test:extra-editor:browser
EXTRA_EDITOR_BROWSERS=chromium,firefox bun run test:extra-editor:browser
```

The browser script defaults to Chromium and accepts optional
`EXTRA_EDITOR_CHROMIUM_EXECUTABLE` / `EXTRA_EDITOR_FIREFOX_EXECUTABLE` paths for
environments with an existing browser cache. It builds the fixture assets
itself, binds an ephemeral localhost port, and closes browsers/server on exit.

Chromium and Firefox desktop checks and Chromium 320px touch-emulation checks
pass. Typecheck and production build pass. Firefox testing caught delayed
native `reportValidity()` focus stealing after blocked mode/history actions;
those barriers now use inline errors and explicit focus, while
form submission keeps native constraint validation. Final implementation review
found no remaining blockers. Number drafts normalize on successful blur/flush
so formatting-only changes such as `1.0` do not leave a phantom dirty state.
Screen-reader speech and physical mobile devices still need manual
verification; these browser checks validate semantics, keyboard behavior,
focus, native validation, touch interaction, and layout, not speech output.

## Save preservation fixes

Valid JSON uses raw mode when strings or property names contain CR/LF, or a
number would change value after JavaScript parsing and serialization. Decimal
tokens are compared exactly before choosing structured mode; equivalent formats
such as `1.0` and `10e-1` remain supported. Structured number drafts reject
precision loss. Unit and browser regressions cover unchanged and edited raw
submissions, nested large integers, precise decimals, line breaks, and invalid
number drafts. Editor announcements and their unused state/styles were removed
intentionally; focus handling and inline validation remain.
