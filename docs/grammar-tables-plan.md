# Grammar tables: implementation plan

## Goal

Grammar tables show the inflected forms of a dictionary word.  A table has
nested row and column headings, and each visible cell contains a Lexurgy sound
change program.  A table is applicable to one or more required word classes
and, optionally, a set of required word categories.

The feature must keep normal word pages responsive.  It therefore caches
Lexurgy results, deduplicates equivalent cell programs, limits global concurrency, and
degrades a slow table to a link for a dedicated full-render page.

This is a new feature alongside, rather than a variant of,
`phonology_tables`.  The two kinds of table share editing mechanics but have
different persisted bodies and renderers.

## Product contract

### Applicability

A grammar table has:

- one or more **required word classes**;
- zero or more **required categories**.

It applies to a word when the word's class is one of the table's classes and
the word has every category selected on the table.  No empty-class, generic
table is permitted.  A table with no categories applies to every word in its
selected classes.

Several tables may match one word.  Render all of them, ordered by table
position.  This permits, for example, a general noun paradigm and a second
paradigm restricted to feminine nouns.

### Lexurgy input and output

Start with `word.word` and pass that spelling to Lexurgy. If the language has
an IPA estimator, run each inflected spelling through it afterwards for display.
Stored `word.ipa` is placeholder data only, not the paradigm input. The cache
source kind is therefore always `spelling`.

Before Lexurgy runs, table source expands `%%{word}`, `%%{ipa}`, and
`%%{extra.path}` placeholders for the current word. `extra.path` traverses
objects with dot segments and arrays with numeric segments; scalar values become
plain text and arrays or objects become compact JSON. A missing or malformed
placeholder is a rendering error rather than silently producing bad rules.

The same directives are available to an IPA estimator, in `src/placeholders.rs`
so both expanders stay one implementation. There, `%%{word}` is the word being
estimated, and `%%{ipa}` and `%%{extra.path}` resolve only where the caller has
a word record behind the estimate: a word form or a grammar table, but not free
translation text. Because `%%{word}` varies per input, an estimator with
directives runs one Lexurgy program per distinct expanded source, capped so a
long translation cannot fan out without bound; a directive-free estimator is
still a single request.

The table editor's cell previews take this same path: they run table source on
the example spelling, then estimate the rendered form's IPA for display.
Previews may supply the selected example's IPA and extra data for placeholders.

Each grammar table has a shared Lexurgy `preamble` and each cell has its own
`changes`.  The evaluator runs the exact composed source:

```text
<preamble>\n<cell changes>
```

An empty cell program runs the preamble alone; it is identity without a Lexurgy
request only when the preamble is also empty.  This gives the preamble one
unambiguous meaning even if it contains an all-cell transformation.  Cells
hidden beneath a merge are never evaluated.

### Time budgets and fallback

The normal word page must not wait indefinitely for inflection.  It loads the
grammar section asynchronously.  The cheap matching query is inside a short,
configurable section budget; once it completes, start all table evaluators from
the same section `Instant`.  Every table gets a deadline of
`min(section_deadline, table_start + per_table_budget)`, including cache lookup,
waiting for a Lexurgy concurrency permit, and cache misses.  Collect completed
table results as they arrive, preserve the original table ordering, and mark
only unfinished tables timed out at the section deadline.  Do not wrap a
`join_all` in one timeout, because it would discard useful completed results.
If the matching query itself exhausts the section budget, return one friendly
section-level error and leave the word page otherwise usable.

When a table runs out of its normal-page budget, replace that table with:

> this table took too long to render
>
> view as full page

The link preserves both the word and table identity, for example:

`/languages/{code}/words/{slug}/{lemma}/grammar-tables/{table_id}`

This is a word-specific full-render page; the standalone table definition page
does not itself know which word to inflect.  It has a larger configurable
budget.  On its own timeout, show:

> this took too long to render. try reducing the rule count or reducing the
> number of unique cells.

Initial values should be deliberately conservative and configurable, for
example 2 seconds per normal-page table, 4 seconds for its complete grammar
section, and 20 seconds for the full-render page.  Tune these after observing
real paradigms; do not hard-code them into handlers.

Invalid Lexurgy source is an error, not a timeout.  Show a concise error on the
word page and the detailed Lexurgy error—with cell coordinate and preamble/cell
line offset—on the full-render page.  Cache only successful output.

## Data model and migration

Create `migrations/036_grammar_tables.sql` (use the next migration number at
implementation time if the sequence has advanced):

```sql
create table grammar_tables (
    id uuid primary key default uuidv7(),
    language_id uuid not null references languages(id) on delete cascade,
    name text not null,
    description text not null default '',
    preamble text not null default '',
    body jsonb not null,
    position integer not null default 0,
    schema_version integer not null default 1,
    created_at timestamptz not null default current_timestamp,
    updated_at timestamptz not null default current_timestamp,
    created_by uuid references users(id) on delete set null,
    updated_by uuid references users(id) on delete set null
);

create index grammar_tables_language_position_idx
    on grammar_tables (language_id, position);

create table grammar_table_word_classes (
    grammar_table_id uuid not null references grammar_tables(id) on delete cascade,
    word_class_id uuid not null references word_classes(id) on delete restrict,
    primary key (grammar_table_id, word_class_id)
);

create index grammar_table_word_classes_class_idx
    on grammar_table_word_classes (word_class_id);

create table grammar_table_categories (
    grammar_table_id uuid not null references grammar_tables(id) on delete cascade,
    category_id uuid not null references word_categories(id) on delete restrict,
    primary key (grammar_table_id, category_id)
);

create index grammar_table_categories_category_idx
    on grammar_table_categories (category_id);

alter type auditable_resource add value 'grammar_table';
```

Use relation tables rather than UUID arrays so foreign keys, indexing, and
cross-language validation remain explicit.  `restrict` on class/category
deletion avoids silently leaving an invalid table with no class or an altered
scope.  Deletion UI must report that the class/category is used by a grammar
table and link to the relevant table.

The repository validates transactionally that:

1. the table has at least one word class;
2. every class and category belongs to the table's language;
3. the body is structurally valid before associations/body are committed.

Add a durable, content-addressed result cache in the same migration:

```sql
create table grammar_render_cache (
    runner_version integer not null,
    source_kind text not null check (source_kind in ('ipa', 'spelling')),
    changes_hash text not null,
    input_word text not null,
    output_word text not null,
    created_at timestamptz not null default current_timestamp,
    primary key (runner_version, source_kind, changes_hash, input_word)
);
```

`changes_hash` is SHA-256 of the exact composed source.  The primary key makes
cache entries independent of a grammar-table ID: identical rules and input can
be reused between cells and tables.  A bumped `runner_version` deliberately
invalidates all prior rows when Lexurgy behavior or request semantics change.

Start with a 90-day TTL.  A read treats an older row as a miss; its write must
use `on conflict (...) do update set output_word = excluded.output_word,
created_at = current_timestamp`, so an expired row is refreshed even if the
maintenance job has not run.  Add a scheduled maintenance query to delete
expired rows, but correctness must not depend on that scheduler existing.

## Rust types and repositories

Add `src/model/grammar_tables.rs` and export it from `src/model/mod.rs`.

Use dedicated types rather than adapting `PhonologyTable::Body`:

```rust
struct GrammarBody {
    rows: Vec<GrammarRow>,
    columns: Vec<GrammarColumn>,
}

struct GrammarCell {
    changes: String,
    rowspan: u32, // defaults to 1
    colspan: u32, // defaults to 1
}
```

`GrammarRow` and `GrammarColumn` retain the current `Group`/`Individual`,
heading, `autogenerated`, and nested-child shape.  Keep span validation equal
to phonology tables: every leaf row owns one cell per leaf column; spans are
positive, bounded, non-overlapping; cells hidden under spans have empty
programs and 1×1 spans.  Enforce these initial limits both on save and
defensively before render: depth <= 6, <= 50 leaf rows, <= 50 leaf columns,
<= 2,500 leaf cells, <= 256 unique composed programs, preamble <= 32 KiB, cell
program <= 16 KiB, and total composed-program bytes <= 256 KiB.  Make the
limits configuration values if real language data needs adjustment.

`GrammarTableRepository` should own:

- create/update/get/list/delete/swap with the same language Editor permission
  model as phonology tables;
- `matching_for_word(&Word)`, using SQL to select tables matching the word
  class and to exclude tables with a required category missing from the word;
- `get_matching_table_for_word`, used by the full-render route to prevent
  arbitrary table/word combinations;
- relation loading and cross-language validation;
- cache bulk read and freshness-preserving upsert writes.

Use nullable `created_by`/`updated_by` with `on delete set null`; repositories,
templates, and API shapes must tolerate a deleted author.  Preserve the
project's existing ban/permission checks on all mutations.  Add `grammar_table`
to the auditable-resource enum and record create/update/delete actions under
the existing audit-log conventions.

Create and update accept complete class/category lists, not patch semantics:
the lists are required JSON/form fields, and an update replaces each relation
set by delete-and-insert inside the same transaction as the body update.  A
missing or empty class list is rejected.  The matching SQL is explicitly:
`language_id = word.language`, `exists` a selected class equal to
`word.word_class`, and `not exists` any required category absent from
`word_word_categories`; order by `(position, id)`.  A word with no class has no
matches.  Class/category deletion must preflight the relation tables to give a
helpful list/link, then map a raced `restrict` FK violation to the same error.

## Evaluator and cache coordinator

Create `src/grammar.rs` (or `src/grammar_tables/evaluator.rs`) with a
testable evaluator boundary instead of placing the orchestration in an Axum
handler.  It composes a display-ready `RenderedGrammarTable` from a word and a
grammar table.

The algorithm is:

1. normalize the selected spelling input and set `source_kind` to `spelling`;
2. flatten visible, anchor cells while retaining their grid coordinates;
3. return identity output immediately only when both the preamble and cell
   program are blank;
4. deduplicate remaining work by `(source_kind, composed_changes, input_word)`;
5. bulk-load every cache key in one SQL query;
6. group cache misses by identical composed changes;
7. for one table/word evaluation, send one Lexurgy request per unique composed
   program; the shared input is identical, so multi-input batching does not
   help at this level;
8. map each returned output back to every original cell coordinate;
9. validate each response, then upsert each successful group immediately and
   return the rendered cell map.

Never make one outbound request for every repeated cell program.  Within a
single word/table render, concurrency is therefore across unique programs and
outputs are mapped by stored coordinate, never task-completion order.  A future
collection-level evaluator may flatten all matching tables and batch their
different source words by identical program, but that is explicitly out of v1;
do not add a misleading `max_input_words_per_request` setting yet.

Before mapping a successful HTTP response, require `response.errors` to be
absent/empty and `response.output_words.len() == request.input_words.len()`.
Anything else is a non-cacheable table error.  The first failed program makes
the current table an error rather than a partial table, but successful groups
already completed before that failure remain cached for the later full-page
retry.

Add a `GrammarRuntime` to `AppState`, initialized in `main.rs`.  It contains an
`Arc<dyn LexurgyRunner>` and a process-wide
`Arc<tokio::sync::Semaphore>`; acquire the latter before every outbound grammar
Lexurgy call.  This is global, not just per HTTP request.  The runner trait
returns the existing Lexurgy response/error types so HTTP tests can inject a
fake runner and make timing/cache assertions without a live Lexurgy process.

Because `AppState` currently has a hand-written `Clone` and many direct struct
literals, first introduce `AppState::new(pool, email_service)` and a test
builder/`with_grammar_runtime` override, then migrate every existing literal.
Keep a manual `Debug` implementation that omits the trait object.  Add a
`GrammarConfig: Default` behind `#[serde(default)]`, and update the test config
literal, so old config files and tests continue to load.  Its defaults include:

- `grammar_lexurgy_concurrency` (initially 4);
- normal-page per-table and total budgets;
- full-render budget;
- grammar cache TTL and `runner_version`.

Use `tokio::time::timeout_at` with one absolute `Instant` propagated through
the evaluator.  Do not reset the timer between cache lookup, semaphore wait,
and network calls.  The normal-page parent applies both its per-table and
section deadlines; the full page supplies the larger deadline.  Dropping the
future cancels the app-side request and releases its permit, but it cannot by
itself guarantee that the remote Lexurgy service stopped running.  Document
this as an app-side concurrency bound; verify Lexurgy disconnect behavior in a
separate integration check before claiming it bounds remote work.  Do not
launch background warmup work in v1: it would defeat the load protection.  A
successful full-render visit warms the durable cache naturally.

Do not add process-local singleflight in v1.  The durable cache safely
converges concurrent cold requests, while a correct leader/follower lifecycle
(leader timeout, follower deadlines, cleanup, and remote permits) is more
complex than the initial feature needs.  Revisit it only if cache-miss metrics
show a real stampede.

Refactor the minimal shared Lexurgy request path out of `src/lexurgy.rs` behind
`LexurgyRunner`.  Preserve its existing response/error types and 60-second
outer network timeout; the grammar budget will generally be smaller and wins
first.

## Routes, templates, and frontend

### CRUD

Mirror the phonology-table feature:

- model: `src/model/grammar_tables.rs`;
- HTML controller: `src/controller/html/grammar_tables.rs`;
- API controller: `src/controller/api/grammar_tables.rs`;
- templates under `templates/languages/grammar-tables/`;
- API/CLI resource wiring after the HTTP feature is complete;
- language footer/link and language-page list section.

The metadata step selects required classes and optional categories using the
existing language-local class/category data.  The body step edits preamble and
grid.  The standalone grammar-table page renders headings, applicability, and
rule summaries; it never runs a lemma by itself.

### Shared grid editor

Extract only grid mechanics from `frontend/src/page/languages/phonology-editor/`:

- row/column trees and leaf counting;
- paths, focus movement, keyboard behavior;
- insertion/removal, grouping, merging, and span repair;
- the header and row structural components.

Keep cell payloads behind an adapter so the phonology editor remains behaviorally
unchanged.  Its adapter retains `phonemes` and annotations.  The grammar adapter
uses `{ changes, rowspan, colspan }`.

Grammar cells should show a short first-line/empty summary and open a modal
CodeMirror editor with the existing Lexurgy language mode.  The modal edits one
cell's `changes`; it must not execute sound changes while typing.  Add a
separate CodeMirror field for the shared preamble.  A cell-only document may
reference declarations in the preamble, so either lint the virtual
`preamble + cell` document and remap diagnostics to the cell, or use
highlighting-only support in the modal; do not show false undefined-reference
errors.  Destroy CodeMirror views on modal close and commit one cell save as a
single outer reducer history action.  Reuse the existing modal CSS and
sound-change syntax support.  Preserve compatibility exports such as
`ControlButton`, which other frontend code imports from the current phonology
editor path.

### Word and full-render pages

Extend `LemmaTemplate`/`templates/words/lemma.html` with one lightweight
`#grammar-tables` loading container, not server-blocking evaluation or
per-table server placeholders.  Add a new frontend entrypoint that fetches a
word-specific endpoint and creates completed-table, error, or timeout-fallback
nodes from its ordered result list.  Add the entrypoint to `frontend/build.ts`,
its module script tag to the lemma template, and any required CSS explicitly.

Suggested endpoints:

```text
GET /api/languages/{code}/words/{slug}/{lemma}/grammar-tables
GET /api/languages/{code}/words/{slug}/{lemma}/grammar-tables/{table_id}
GET /languages/{code}/words/{slug}/{lemma}/grammar-tables/{table_id}
```

The collection API gives each table its normal-page budget and returns an
ordered, per-table tagged result (`rendered`, `timed_out`, or `error`); a
timed-out table includes the full-render URL.  Its `rendered` variant contains
one server-rendered, escaped HTML fragment generated by a canonical Rust
grammar-table renderer.  The async frontend inserts that fragment only after
the normal trusted same-origin fetch; the full-render HTML page uses the exact
same renderer inside its Askama wrapper.  Do not maintain separate nested-grid
renderers in TypeScript and Rust.

The item API and HTML full-render page use the larger budget.  They must verify
that the table exists in the language and matches the current word.  Keep rule
output, headings, and error text escaped in the canonical renderer; never
concatenate unescaped rule output into HTML.

Use an accessible loading status.  Provide a noscript explanation and link to
the static grammar-table definition; generated forms require JavaScript on the
ordinary word page.

## Delivery order

1. Write the migration, Rust body validation, repository CRUD/matching, and
   model tests, including the audit-log resource wiring.  No Lexurgy calls yet.
2. Extract the frontend structural grid into shared code, prove the existing
   phonology editor still passes its tests, then build the grammar editor and
   CRUD pages.
3. Introduce the `AppState` runtime constructor/test builder, then implement
   the evaluator, cache schema access, absolute deadlines, global semaphore,
   and a fake-runner test suite.
4. Add word-page placeholders, collection/item APIs, and the full-render HTML
   route; then add timeout and error states.
5. Wire navigation, API CLI support, metrics, maintenance, documentation, and
   rollout checks.

Do not merge the editor extraction with an unrelated visual rewrite.  It is the
highest regression-risk part, so keep the phonology behavior tests green before
adding grammar-specific cells.

## Tests and acceptance criteria

### Model and repository

- reject empty class selection, out-of-language class/category IDs, invalid
  bodies, invalid spans, over-depth/over-cardinality bodies, and oversize
  programs;
- matching requires a selected class and all required categories;
- multiple matching tables use `(position, id)` ordering, and a no-class word
  matches none;
- class/category deletion reports a useful in-use failure and survives an FK
  race; deleted authors render safely as absent;
- editor permission, banned-user, and cross-language access tests mirror
  phonology/sound-change tables; audit-log actions are recorded.

### Evaluator

- spelling is always the grammar-rule input; when an estimator exists, its
  output for each inflected spelling is normalized and displayed as IPA;
- a blank cell plus empty preamble is identity and makes no runner call; a
  blank cell with a preamble runs the preamble;
- merged cells and repeated programs map one output to every intended cell;
- repeated programs make one call; different programs respect the global
  concurrency cap and map output independently of completion order;
- warm cache makes no Lexurgy call; cache key changes when input, source kind,
  source, or runner version changes; an expired row refreshes correctly even
  without the cleanup job;
- cache failures, Lexurgy `errors`, and malformed response cardinality are not
  persisted; completed successful program groups persist before a later group
  fails/times out;
- table and section deadlines include matching-query and queued-semaphore time,
  preserve already-completed ordered tables, and yield the specified fallback;
  full page gets its larger deadline;
- cancellation cleans up local permits/runtime state; an integration check
  documents actual Lexurgy behavior after client disconnect.

### HTTP and UI

- grammar CRUD and selection forms validate/authorize correctly;
- word API exposes only tables matching that word and language;
- normal page renders independently of a slow grammar computation;
- timeout fallback has the correct word-specific full-render URL;
- full-render page distinguishes timeout from a Lexurgy rule error;
- collection and full-render pages use the same escaped nested-grid fragment;
- headings, Lexurgy output, and error text cannot inject HTML;
- keyboard navigation, merges, undo/redo, and serialization still work in both
  the phonology and grammar editor; preamble-aware cell diagnostics do not
  falsely reject shared declarations.

### Operations

Instrument cache hit/miss, expired-cache refresh, unique program count, number
of Lexurgy calls, queue wait, execution duration, per-page timeout, and
full-page timeout.  Use these to tune budgets/concurrency after deployment.  A
cold table that regularly times out should be visible in metrics without
logging source words or rules.

Run the relevant Rust tests through the shared target wrapper, e.g.
`just cargo-agent start test grammar_tables`, and inspect its logs/status rather
than fighting the developer's Cargo lock.
