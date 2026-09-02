//! Bounded Lexurgy evaluation for grammar-table cells.
//!
//! The evaluator is intentionally independent of Axum.  HTTP handlers choose
//! their deadline, while this module makes cache lookup, semaphore wait, and
//! outbound Lexurgy work all consume that same absolute deadline.

use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, LazyLock},
};

use askama::filters::{Html, escape};
use async_trait::async_trait;
use futures::{StreamExt, stream::FuturesUnordered};
use serde::Serialize;
use tokio::{
    sync::Semaphore,
    time::{Instant, timeout_at},
};

use crate::{
    config::CONFIG,
    lexurgy::{self, Request, Response},
    model::{
        grammar_tables::{GrammarCacheKey, GrammarTable, GrammarTableRepository, compose_changes},
        sound_change_sets::SoundChangeSetRepository,
        words::Word,
    },
    placeholders::{self, Placeholders},
    util::stable_hash,
};

static GRAMMAR_LEXURGY_PERMITS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(CONFIG.grammar.lexurgy_concurrency.max(1))));

/// Estimate IPA for already-inflected forms. Grammar rules always start from a
/// word's spelling; the estimator is a display-only second pass so it cannot
/// change the input to the table's inflection rules.
pub async fn estimate_ipa(
    sets: &SoundChangeSetRepository,
    ipa_estimator: Option<uuid::Uuid>,
    input_words: Vec<String>,
    values: &Placeholders<'_>,
    deadline: Instant,
) -> Result<Option<Vec<String>>, GrammarRenderError> {
    estimate_ipa_with_permits(
        sets,
        ipa_estimator,
        input_words,
        values,
        deadline,
        GRAMMAR_LEXURGY_PERMITS.clone(),
    )
    .await
}

async fn estimate_ipa_with_permits(
    sets: &SoundChangeSetRepository,
    ipa_estimator: Option<uuid::Uuid>,
    input_words: Vec<String>,
    values: &Placeholders<'_>,
    deadline: Instant,
    permits: Arc<Semaphore>,
) -> Result<Option<Vec<String>>, GrammarRenderError> {
    let Some(ipa_estimator) = ipa_estimator else {
        return Ok(None);
    };
    let response = timeout_at(
        deadline,
        sets.run_estimator_bounded(&ipa_estimator, input_words.clone(), values, permits),
    )
    .await
    .map_err(|_| GrammarRenderError::TimedOut)?
    .map_err(|error| {
        GrammarRenderError::Failed(format!("IPA estimation failed: {}", error.message))
    })?;
    if let Some(errors) = response.errors.filter(|errors| !errors.is_empty()) {
        return Err(GrammarRenderError::Failed(format!(
            "IPA estimation failed: {}",
            errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if response.output_words.len() != input_words.len() {
        return Err(GrammarRenderError::Failed(
            "IPA estimator returned an unexpected number of outputs.".into(),
        ));
    }
    Ok(Some(response.output_words))
}

/// Expand word data in a grammar-table Lexurgy program.  Values are inserted
/// as source text, not escaped, so authors can use them in Lexurgy syntax.
///
/// Supported placeholders are `%%{word}`, `%%{ipa}`, and `%%{extra.path}`.
fn expand_placeholders(source: &str, word: &Word) -> Result<String, GrammarRenderError> {
    placeholders::expand(source, &Placeholders::for_word(word)).map_err(GrammarRenderError::Failed)
}

/// Expand the directives available while editing a grammar table. The selected
/// example may include saved word data, allowing previews to behave like the
/// rendered table for `%%{ipa}` and `%%{extra.*}` directives.
fn expand_preview_placeholders(
    source: &str,
    values: &Placeholders<'_>,
) -> Result<String, GrammarRenderError> {
    placeholders::expand(source, values).map_err(GrammarRenderError::Failed)
}

#[async_trait]
pub trait LexurgyRunner: Send + Sync {
    async fn run(
        &self,
        request: Request,
    ) -> crate::err::AppResult<Result<Response, lexurgy::Error>>;
}

#[derive(Debug)]
pub struct HttpLexurgyRunner;

#[async_trait]
impl LexurgyRunner for HttpLexurgyRunner {
    async fn run(
        &self,
        request: Request,
    ) -> crate::err::AppResult<Result<Response, lexurgy::Error>> {
        lexurgy::send_scv1(&request).await
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedGrammarCell {
    pub row: usize,
    pub column: usize,
    pub value: String,
    pub ipa: Option<String>,
    pub rowspan: u32,
    pub colspan: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedGrammarTable {
    pub table_id: uuid::Uuid,
    pub cells: Vec<RenderedGrammarCell>,
}

fn esc(value: &str) -> impl std::fmt::Display + '_ {
    escape(value, Html).expect("Askama HTML escaping is infallible")
}

/// The one grammar-grid renderer shared by the JSON fragment and the full
/// page.  It deliberately emits only escaped dynamic text.
pub fn render_html(
    table: &GrammarTable,
    rendered: &RenderedGrammarTable,
) -> Result<String, GrammarRenderError> {
    render_html_with_edit_links(table, rendered, None)
}

/// Render a grammar table's stored paradigm for listings, where there is no
/// word available to evaluate the cell rules against. This deliberately uses
/// the same table structure as a rendered paradigm, but shows each cell's
/// source so the table remains useful outside a word page.
pub fn render_definition_html(
    table: &GrammarTable,
    edit_links: Option<(&str, &str)>,
) -> Result<String, GrammarRenderError> {
    use crate::model::grammar_tables::{GrammarCell, GrammarColumn, GrammarRow};

    struct HeaderCell {
        heading: String,
        colspan: usize,
        rowspan: usize,
    }

    fn count_column_leaves(column: &GrammarColumn) -> usize {
        match column {
            GrammarColumn::Individual { .. } => 1,
            GrammarColumn::Group { columns, .. } => columns.iter().map(count_column_leaves).sum(),
        }
    }

    fn max_column_depth(columns: &[GrammarColumn], depth: usize) -> usize {
        columns
            .iter()
            .fold(depth, |max_depth, column| match column {
                GrammarColumn::Individual { .. } => max_depth,
                GrammarColumn::Group { columns, .. } => {
                    max_depth.max(max_column_depth(columns, depth + 1))
                }
            })
    }

    fn count_row_leaves(rows: &[GrammarRow]) -> usize {
        rows.iter()
            .map(|row| match row {
                GrammarRow::Group { rows, .. } => count_row_leaves(rows),
                GrammarRow::Individual { .. } => 1,
            })
            .sum()
    }

    fn max_row_depth(rows: &[GrammarRow], depth: usize) -> usize {
        rows.iter().fold(depth, |max_depth, row| match row {
            GrammarRow::Individual { .. } => max_depth,
            GrammarRow::Group { rows, .. } => max_depth.max(max_row_depth(rows, depth + 1)),
        })
    }

    fn write_rows(
        html: &mut String,
        rows: &[GrammarRow],
        pending_groups: &mut Vec<(String, usize)>,
        max_row_depth: usize,
        current_depth: usize,
        leaf_row: &mut usize,
        visible_cells: &HashMap<(usize, usize), &GrammarCell>,
    ) {
        for row in rows {
            match row {
                GrammarRow::Group { heading, rows, .. } => {
                    pending_groups.push((heading.clone(), count_row_leaves(rows)));
                    write_rows(
                        html,
                        rows,
                        pending_groups,
                        max_row_depth,
                        current_depth + 1,
                        leaf_row,
                        visible_cells,
                    );
                }
                GrammarRow::Individual { heading, cells, .. } => {
                    html.push_str("<tr>");
                    for (group_heading, rowspan) in pending_groups.drain(..) {
                        write!(html, "<th").unwrap();
                        if rowspan > 1 {
                            write!(html, " rowspan=\"{rowspan}\"").unwrap();
                        }
                        write!(html, ">{}</th>", esc(&group_heading)).unwrap();
                    }

                    let colspan = max_row_depth - current_depth;
                    write!(html, "<th").unwrap();
                    if colspan > 1 {
                        write!(html, " colspan=\"{colspan}\"").unwrap();
                    }
                    write!(html, ">{}</th>", esc(heading)).unwrap();

                    for (column, _) in cells.iter().enumerate() {
                        let Some(cell) = visible_cells.get(&(*leaf_row, column)) else {
                            continue;
                        };
                        write!(html, "<td").unwrap();
                        if cell.rowspan > 1 {
                            write!(html, " rowspan=\"{}\"", cell.rowspan).unwrap();
                        }
                        if cell.colspan > 1 {
                            write!(html, " colspan=\"{}\"", cell.colspan).unwrap();
                        }
                        write!(html, "><code>{}</code></td>", esc(&cell.changes)).unwrap();
                    }
                    html.push_str("</tr>");
                    *leaf_row += 1;
                }
            }
        }
    }

    let body = table
        .body()
        .map_err(|error| GrammarRenderError::Failed(error.to_string()))?;
    let max_column_depth = max_column_depth(&body.columns, 1);
    let max_row_depth = max_row_depth(&body.rows, 1);
    let mut header_rows: Vec<Vec<HeaderCell>> = (0..max_column_depth).map(|_| Vec::new()).collect();
    let mut pending_columns: Vec<(&GrammarColumn, usize)> = body
        .columns
        .iter()
        .rev()
        .map(|column| (column, 0))
        .collect();

    while let Some((column, depth)) = pending_columns.pop() {
        match column {
            GrammarColumn::Group {
                heading, columns, ..
            } => {
                header_rows[depth].push(HeaderCell {
                    heading: heading.clone(),
                    colspan: count_column_leaves(column),
                    rowspan: 1,
                });
                for child in columns.iter().rev() {
                    pending_columns.push((child, depth + 1));
                }
            }
            GrammarColumn::Individual { heading, .. } => {
                header_rows[depth].push(HeaderCell {
                    heading: heading.clone(),
                    colspan: 1,
                    rowspan: max_column_depth - depth,
                });
            }
        }
    }

    let visible_cells: HashMap<_, _> = body
        .visible_cells()
        .into_iter()
        .map(|(row, column, cell)| ((row, column), cell))
        .collect();
    let mut html = String::new();
    write!(
        html,
        "<section class=\"grammar-table-container\" aria-labelledby=\"grammar-table-{}\">",
        table.id
    )
    .unwrap();
    if let Some((edit_metadata_link, edit_body_link)) = edit_links {
        write!(html, "<div class=\"header-with-actions\"><h2 id=\"grammar-table-{}\">{}</h2><ul><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit metadata</span></a></li><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit table body</span></a></li></ul></div>", table.id, esc(&table.name), esc(edit_metadata_link), esc(edit_body_link)).unwrap();
    } else {
        write!(
            html,
            "<h2 id=\"grammar-table-{}\">{}</h2>",
            table.id,
            esc(&table.name)
        )
        .unwrap();
    }

    html.push_str("<div class=\"grammar-table-scroll\"><table class=\"grammar-table\"><thead>");
    for (index, row) in header_rows.iter().enumerate() {
        html.push_str("<tr>");
        if index == 0 {
            write!(
                html,
                "<th colspan=\"{max_row_depth}\" rowspan=\"{max_column_depth}\"></th>"
            )
            .unwrap();
        }
        for cell in row {
            write!(html, "<th").unwrap();
            if cell.colspan > 1 {
                write!(html, " colspan=\"{}\"", cell.colspan).unwrap();
            }
            if cell.rowspan > 1 {
                write!(html, " rowspan=\"{}\"", cell.rowspan).unwrap();
            }
            write!(html, ">{}</th>", esc(&cell.heading)).unwrap();
        }
        html.push_str("</tr>");
    }
    html.push_str("</thead><tbody>");
    write_rows(
        &mut html,
        &body.rows,
        &mut Vec::new(),
        max_row_depth + 1,
        1,
        &mut 0,
        &visible_cells,
    );
    html.push_str("</tbody></table></div></section>");
    Ok(html)
}

/// The word-page fragment stays deliberately compact.  The dedicated table
/// page may additionally give language editors a way back to the editor,
/// matching the phonology-table page's header actions.
pub fn render_html_with_edit_links(
    table: &GrammarTable,
    rendered: &RenderedGrammarTable,
    edit_links: Option<(&str, &str)>,
) -> Result<String, GrammarRenderError> {
    use crate::model::grammar_tables::{GrammarColumn, GrammarRow};

    struct HeaderCell {
        heading: String,
        colspan: usize,
        rowspan: usize,
    }

    fn count_column_leaves(column: &GrammarColumn) -> usize {
        match column {
            GrammarColumn::Individual { .. } => 1,
            GrammarColumn::Group { columns, .. } => columns.iter().map(count_column_leaves).sum(),
        }
    }

    fn count_row_leaves(rows: &[GrammarRow]) -> usize {
        rows.iter()
            .map(|row| match row {
                GrammarRow::Group { rows, .. } => count_row_leaves(rows),
                GrammarRow::Individual { .. } => 1,
            })
            .sum()
    }

    fn max_column_depth(columns: &[GrammarColumn], depth: usize) -> usize {
        columns
            .iter()
            .fold(depth, |max_depth, column| match column {
                GrammarColumn::Individual { .. } => max_depth,
                GrammarColumn::Group { columns, .. } => {
                    max_depth.max(max_column_depth(columns, depth + 1))
                }
            })
    }

    fn max_row_depth(rows: &[GrammarRow], depth: usize) -> usize {
        rows.iter().fold(depth, |max_depth, row| match row {
            GrammarRow::Individual { .. } => max_depth,
            GrammarRow::Group { rows, .. } => max_depth.max(max_row_depth(rows, depth + 1)),
        })
    }

    fn write_rows(
        html: &mut String,
        rows: &[GrammarRow],
        pending_groups: &mut Vec<(String, usize)>,
        max_row_depth: usize,
        current_depth: usize,
        leaf_row: &mut usize,
        values: &HashMap<(usize, usize), &RenderedGrammarCell>,
    ) {
        for row in rows {
            match row {
                GrammarRow::Group { heading, rows, .. } => {
                    pending_groups.push((heading.clone(), count_row_leaves(rows)));
                    write_rows(
                        html,
                        rows,
                        pending_groups,
                        max_row_depth,
                        current_depth + 1,
                        leaf_row,
                        values,
                    );
                }
                GrammarRow::Individual { heading, cells, .. } => {
                    html.push_str("<tr>");
                    for (group_heading, rowspan) in pending_groups.drain(..) {
                        write!(html, "<th").unwrap();
                        if rowspan > 1 {
                            write!(html, " rowspan=\"{rowspan}\"").unwrap();
                        }
                        write!(html, ">{}</th>", esc(&group_heading)).unwrap();
                    }
                    let colspan = max_row_depth - current_depth;
                    write!(html, "<th").unwrap();
                    if colspan > 1 {
                        write!(html, " colspan=\"{colspan}\"").unwrap();
                    }
                    write!(html, ">{}</th>", esc(heading)).unwrap();

                    for column in 0..cells.len() {
                        let Some(cell) = values.get(&(*leaf_row, column)) else {
                            continue;
                        };
                        write!(html, "<td").unwrap();
                        if cell.rowspan > 1 {
                            write!(html, " rowspan=\"{}\"", cell.rowspan).unwrap();
                        }
                        if cell.colspan > 1 {
                            write!(html, " colspan=\"{}\"", cell.colspan).unwrap();
                        }
                        write!(
                            html,
                            "><span class=\"grammar-cell-word\">{}</span>",
                            esc(&cell.value)
                        )
                        .unwrap();
                        if let Some(ipa) = &cell.ipa {
                            write!(html, "<span class=\"grammar-cell-ipa\">{}</span>", esc(ipa))
                                .unwrap();
                        }
                        html.push_str("</td>");
                    }
                    html.push_str("</tr>");
                    *leaf_row += 1;
                }
            }
        }
    }

    let body = table
        .body()
        .map_err(|error| GrammarRenderError::Failed(error.to_string()))?;
    let max_column_depth = max_column_depth(&body.columns, 1);
    let max_row_depth = max_row_depth(&body.rows, 1);
    let mut header_rows: Vec<Vec<HeaderCell>> = (0..max_column_depth).map(|_| Vec::new()).collect();
    let mut pending_columns: Vec<(&GrammarColumn, usize)> = body
        .columns
        .iter()
        .rev()
        .map(|column| (column, 0))
        .collect();
    while let Some((column, depth)) = pending_columns.pop() {
        match column {
            GrammarColumn::Group {
                heading, columns, ..
            } => {
                header_rows[depth].push(HeaderCell {
                    heading: heading.clone(),
                    colspan: count_column_leaves(column),
                    rowspan: 1,
                });
                for child in columns.iter().rev() {
                    pending_columns.push((child, depth + 1));
                }
            }
            GrammarColumn::Individual { heading, .. } => header_rows[depth].push(HeaderCell {
                heading: heading.clone(),
                colspan: 1,
                rowspan: max_column_depth - depth,
            }),
        }
    }
    let values: HashMap<_, _> = rendered
        .cells
        .iter()
        .map(|cell| ((cell.row, cell.column), cell))
        .collect();
    let mut html = String::new();
    write!(
        html,
        "<section class=\"grammar-table-container\" aria-labelledby=\"grammar-table-{}\">",
        table.id
    )
    .unwrap();
    if let Some((edit_metadata_link, edit_body_link)) = edit_links {
        write!(html, "<div class=\"header-with-actions\"><h2 id=\"grammar-table-{}\">{}</h2><ul><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit metadata</span></a></li><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit table body</span></a></li></ul></div>", table.id, esc(&table.name), esc(edit_metadata_link), esc(edit_body_link)).unwrap();
    } else {
        write!(
            html,
            "<h2 id=\"grammar-table-{}\">{}</h2>",
            table.id,
            esc(&table.name)
        )
        .unwrap();
    }
    html.push_str("<div class=\"grammar-table-scroll\"><table class=\"grammar-table\"><thead>");
    for (index, row) in header_rows.iter().enumerate() {
        html.push_str("<tr>");
        if index == 0 {
            write!(
                html,
                "<th colspan=\"{max_row_depth}\" rowspan=\"{max_column_depth}\"></th>"
            )
            .unwrap();
        }
        for cell in row {
            write!(html, "<th").unwrap();
            if cell.colspan > 1 {
                write!(html, " colspan=\"{}\"", cell.colspan).unwrap();
            }
            if cell.rowspan > 1 {
                write!(html, " rowspan=\"{}\"", cell.rowspan).unwrap();
            }
            write!(html, ">{}</th>", esc(&cell.heading)).unwrap();
        }
        html.push_str("</tr>");
    }
    html.push_str("</thead><tbody>");
    write_rows(
        &mut html,
        &body.rows,
        &mut Vec::new(),
        max_row_depth + 1,
        1,
        &mut 0,
        &values,
    );
    html.push_str("</tbody></table></div></section>");
    Ok(html)
}

/// Render the listing state for a table that does not yet have an eligible
/// word. Keeping the header here ensures the no-example state retains the
/// same name and editor actions as rendered tables.
pub fn render_no_examples_html(table: &GrammarTable, edit_links: Option<(&str, &str)>) -> String {
    let mut html = String::new();
    write!(
        html,
        "<section class=\"grammar-table-container\" aria-labelledby=\"grammar-table-{}\">",
        table.id
    )
    .unwrap();
    if let Some((edit_metadata_link, edit_body_link)) = edit_links {
        write!(html, "<div class=\"header-with-actions\"><h2 id=\"grammar-table-{}\">{}</h2><ul><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit metadata</span></a></li><li><a class=\"with-icon\" href=\"{}\"><svg class=\"icon\"><use href=\"#icon-edit\"/></svg><span>edit table body</span></a></li></ul></div>", table.id, esc(&table.name), esc(edit_metadata_link), esc(edit_body_link)).unwrap();
    } else {
        write!(
            html,
            "<h2 id=\"grammar-table-{}\">{}</h2>",
            table.id,
            esc(&table.name)
        )
        .unwrap();
    }
    html.push_str("<div class=\"card grammar-table-no-example\">(no examples)</div></section>");
    html
}

pub fn render_example_hint(
    language_code: &str,
    word: &Word,
    first_definition: Option<&str>,
) -> String {
    let href = format!(
        "/languages/{}/words/{}/{}",
        language_code, word.slug, word.lemma
    );
    let mut html = String::from("<p class=\"grammar-table-example\">using ");
    write!(html, "<a href=\"{}\">{}</a>", esc(&href), esc(&word.word)).unwrap();
    if let Some(definition) = first_definition {
        write!(html, " \"{}\"", esc(definition)).unwrap();
    }
    html.push_str(" as an example</p>");
    html
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrammarRenderError {
    TimedOut,
    Failed(String),
}

impl std::fmt::Display for GrammarRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => f.write_str("grammar table rendering timed out"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for GrammarRenderError {}

#[derive(Clone)]
pub struct GrammarEvaluator {
    runner: Arc<dyn LexurgyRunner>,
    permits: Arc<Semaphore>,
    runner_version: i32,
    cache_ttl_seconds: i64,
}

impl Default for GrammarEvaluator {
    fn default() -> Self {
        Self::new(Arc::new(HttpLexurgyRunner))
    }
}

impl GrammarEvaluator {
    pub fn new(runner: Arc<dyn LexurgyRunner>) -> Self {
        Self {
            runner,
            permits: GRAMMAR_LEXURGY_PERMITS.clone(),
            runner_version: CONFIG.grammar.runner_version,
            cache_ttl_seconds: CONFIG.grammar.cache_ttl_seconds,
        }
    }

    #[cfg(test)]
    pub fn with_runtime(
        runner: Arc<dyn LexurgyRunner>,
        permits: Arc<Semaphore>,
        runner_version: i32,
        cache_ttl_seconds: i64,
    ) -> Self {
        Self {
            runner,
            permits,
            runner_version,
            cache_ttl_seconds,
        }
    }

    async fn run_program(
        &self,
        deadline: Instant,
        changes: String,
        input: String,
    ) -> Result<String, GrammarRenderError> {
        let permit = timeout_at(deadline, self.permits.clone().acquire_owned())
            .await
            .map_err(|_| GrammarRenderError::TimedOut)
            .and_then(|result| {
                result.map_err(|_| {
                    GrammarRenderError::Failed("Grammar renderer stopped accepting work.".into())
                })
            })?;
        let request = Request {
            changes,
            input_words: vec![input],
            trace_words: None,
            start_at: None,
            stop_before: None,
            allow_polling: None,
        };
        let response = timeout_at(deadline, self.runner.run(request))
            .await
            .map_err(|_| GrammarRenderError::TimedOut)?
            .map_err(|error| GrammarRenderError::Failed(error.to_string()))?
            .map_err(|error| GrammarRenderError::Failed(error.to_string()))?;
        drop(permit);
        if response
            .errors
            .as_ref()
            .is_some_and(|errors| !errors.is_empty())
        {
            return Err(GrammarRenderError::Failed(
                response.errors.unwrap()[0].message.clone(),
            ));
        }
        if response.output_words.len() != 1 {
            return Err(GrammarRenderError::Failed(
                "Lexurgy returned an unexpected number of outputs.".into(),
            ));
        }
        Ok(response
            .output_words
            .into_iter()
            .next()
            .expect("checked output length"))
    }

    /// Runs the grammar-rule half of an editor preview. IPA estimation happens
    /// afterwards, just as it does for a saved table. These previews
    /// deliberately do not enter the durable table cache: a person can type
    /// many transient rule programs while editing, and only saved table
    /// programs should take up that cache.
    pub async fn preview(
        &self,
        spelling: &str,
        values: &Placeholders<'_>,
        changes: String,
        deadline: Instant,
    ) -> Result<String, GrammarRenderError> {
        let changes = expand_preview_placeholders(&changes, values)?;
        if changes.trim().is_empty() {
            return Ok(spelling.to_owned());
        }
        self.run_program(deadline, changes, spelling.to_owned())
            .await
    }

    pub async fn render(
        &self,
        tables: &GrammarTableRepository,
        sets: &SoundChangeSetRepository,
        ipa_estimator: Option<uuid::Uuid>,
        word: &Word,
        table: &GrammarTable,
        deadline: Instant,
    ) -> Result<RenderedGrammarTable, GrammarRenderError> {
        let body = table
            .body()
            .map_err(|error| GrammarRenderError::Failed(error.to_string()))?;
        let source_kind = "spelling".to_owned();
        let mut cells = Vec::new();
        let mut programs = HashMap::<String, String>::new(); // hash -> exact source
        let mut locations = HashMap::<String, Vec<(usize, usize, u32, u32)>>::new();
        for (row, column, cell) in body.visible_cells() {
            let changes =
                expand_placeholders(&compose_changes(&table.preamble, &cell.changes), word)?;
            if changes.is_empty() {
                cells.push(RenderedGrammarCell {
                    row,
                    column,
                    value: word.word.clone(),
                    ipa: None,
                    rowspan: cell.rowspan,
                    colspan: cell.colspan,
                });
                continue;
            }
            let hash = stable_hash(&changes);
            programs.entry(hash.clone()).or_insert(changes);
            locations
                .entry(hash)
                .or_default()
                .push((row, column, cell.rowspan, cell.colspan));
        }
        let keys: Vec<_> = programs
            .keys()
            .map(|hash| GrammarCacheKey {
                runner_version: self.runner_version,
                source_kind: source_kind.clone(),
                changes_hash: hash.clone(),
                input_word: word.word.clone(),
            })
            .collect();
        let cached = timeout_at(
            deadline,
            tables.cached_outputs(&keys, self.cache_ttl_seconds),
        )
        .await
        .map_err(|_| GrammarRenderError::TimedOut)
        .and_then(|result| result.map_err(|error| GrammarRenderError::Failed(error.to_string())))?;
        let mut outputs = cached;
        let misses: Vec<_> = programs
            .into_iter()
            .filter(|(hash, _)| !outputs.contains_key(hash))
            .collect();
        let evaluator = self.clone();
        let input_word = word.word.clone();
        let mut results: FuturesUnordered<_> = misses
            .into_iter()
            .map(|(hash, changes)| {
                let evaluator = evaluator.clone();
                let input_word = input_word.clone();
                async move {
                    (
                        hash,
                        evaluator.run_program(deadline, changes, input_word).await,
                    )
                }
            })
            .collect();
        let mut first_error = None;
        while let Some((hash, output)) = results.next().await {
            let output = match output {
                Ok(output) => output,
                Err(error) => {
                    first_error.get_or_insert(error);
                    continue;
                }
            };
            let key = GrammarCacheKey {
                runner_version: self.runner_version,
                source_kind: source_kind.clone(),
                changes_hash: hash.clone(),
                input_word: word.word.clone(),
            };
            match timeout_at(deadline, tables.store_cached_output(&key, &output)).await {
                Ok(Ok(())) => {
                    outputs.insert(hash, output);
                }
                Ok(Err(error)) => {
                    first_error
                        .get_or_insert_with(|| GrammarRenderError::Failed(error.to_string()));
                }
                Err(_) => {
                    first_error.get_or_insert(GrammarRenderError::TimedOut);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        for (hash, positions) in locations {
            let output = outputs
                .remove(&hash)
                .ok_or_else(|| GrammarRenderError::Failed("Missing grammar output.".into()))?;
            for (row, column, rowspan, colspan) in positions {
                cells.push(RenderedGrammarCell {
                    row,
                    column,
                    value: output.clone(),
                    ipa: None,
                    rowspan,
                    colspan,
                });
            }
        }
        cells.sort_by_key(|cell| (cell.row, cell.column));
        if let Some(ipa) = estimate_ipa_with_permits(
            sets,
            ipa_estimator,
            cells.iter().map(|cell| cell.value.clone()).collect(),
            &Placeholders::for_word(word),
            deadline,
            self.permits.clone(),
        )
        .await?
        {
            for (cell, ipa) in cells.iter_mut().zip(ipa) {
                cell.ipa = Some(ipa);
            }
        }
        Ok(RenderedGrammarTable {
            table_id: table.id,
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        grammar_tables::{GrammarBody, GrammarCell, GrammarColumn, GrammarRow},
        words::Word,
    };

    fn word(ipa: &str) -> Word {
        Word {
            id: uuid::Uuid::nil(),
            language: uuid::Uuid::nil(),
            word_class: None,
            cognacy: None,
            word: "spelling".into(),
            slug: "spelling".into(),
            lemma: 1,
            ipa: ipa.into(),
            notes: String::new(),
            extra: None,
            like_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            _created_by: None,
            _updated_by: None,
            bookmark: String::new(),
            language_code: None,
            language_name: None,
            word_class_abbreviation: None,
            created_by: None,
            updated_by: None,
        }
    }

    #[test]
    fn table_source_expands_word_data() {
        let mut example = word("ipa");
        example.extra = Some(serde_json::json!({ "stem": { "plural": "stem-pl" } }));

        assert_eq!(
            expand_placeholders("%%{word} %%{ipa} %%{extra.stem.plural}", &example).unwrap(),
            "spelling ipa stem-pl"
        );
        assert!(expand_placeholders("%%{extra.missing}", &example).is_err());
    }

    #[test]
    fn preview_placeholders_use_the_selected_word_data() {
        let extra = serde_json::json!({ "stem": "stem" });
        let values = Placeholders::for_spelling("example")
            .with_ipa(Some("ipa"))
            .with_extra(Some(&extra));
        assert_eq!(
            expand_preview_placeholders("%%{word} %%{ipa} %%{extra.stem}", &values,).unwrap(),
            "example ipa stem"
        );
        assert!(expand_preview_placeholders("%%{extra.missing}", &values).is_err());
    }

    #[test]
    fn definition_renderer_shows_the_source_grid_without_covered_cells() {
        let body = GrammarBody {
            columns: vec![GrammarColumn::Group {
                heading: "number".into(),
                autogenerated: false,
                columns: vec![
                    GrammarColumn::Individual {
                        heading: "singular".into(),
                        autogenerated: false,
                    },
                    GrammarColumn::Individual {
                        heading: "plural".into(),
                        autogenerated: false,
                    },
                ],
            }],
            rows: vec![GrammarRow::Group {
                heading: "case".into(),
                autogenerated: false,
                rows: vec![
                    GrammarRow::Individual {
                        heading: "nominative".into(),
                        autogenerated: false,
                        cells: vec![
                            GrammarCell {
                                changes: "a > & <".into(),
                                rowspan: 2,
                                colspan: 1,
                            },
                            GrammarCell {
                                changes: "a > e".into(),
                                rowspan: 1,
                                colspan: 1,
                            },
                        ],
                    },
                    GrammarRow::Individual {
                        heading: "accusative".into(),
                        autogenerated: false,
                        cells: vec![
                            GrammarCell {
                                changes: String::new(),
                                rowspan: 1,
                                colspan: 1,
                            },
                            GrammarCell {
                                changes: "a > i".into(),
                                rowspan: 1,
                                colspan: 1,
                            },
                        ],
                    },
                ],
            }],
        };
        let table = GrammarTable {
            id: uuid::Uuid::nil(),
            language_id: uuid::Uuid::nil(),
            name: "declension".into(),
            description: String::new(),
            preamble: String::new(),
            body: serde_json::to_value(body).unwrap(),
            position: 0,
            schema_version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: None,
            updated_by: None,
            word_class_ids: vec![],
            category_ids: vec![],
        };

        let html = render_definition_html(&table, None).unwrap();
        assert!(html.contains("<th colspan=\"2\">number</th>"));
        assert!(html.contains("<th rowspan=\"2\">case</th>"));
        assert!(
            html.contains("<td rowspan=\"2\"><code>a &#62; &#38; &#60;</code></td>"),
            "{html}"
        );
        assert_eq!(html.matches("<td").count(), 3);
    }

    #[test]
    fn rendered_cells_show_ipa_below_the_inflected_word() {
        let table = GrammarTable {
            id: uuid::Uuid::nil(),
            language_id: uuid::Uuid::nil(),
            name: "declension".into(),
            description: String::new(),
            preamble: String::new(),
            body: serde_json::to_value(GrammarBody {
                columns: vec![GrammarColumn::Individual {
                    heading: "singular".into(),
                    autogenerated: false,
                }],
                rows: vec![GrammarRow::Individual {
                    heading: "nominative".into(),
                    autogenerated: false,
                    cells: vec![GrammarCell {
                        changes: String::new(),
                        rowspan: 1,
                        colspan: 1,
                    }],
                }],
            })
            .unwrap(),
            position: 0,
            schema_version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: None,
            updated_by: None,
            word_class_ids: vec![],
            category_ids: vec![],
        };
        let rendered = RenderedGrammarTable {
            table_id: table.id,
            cells: vec![RenderedGrammarCell {
                row: 0,
                column: 0,
                value: "form<".into(),
                ipa: Some("[form&]".into()),
                rowspan: 1,
                colspan: 1,
            }],
        };

        let html = render_html(&table, &rendered).unwrap();
        assert!(html.contains("grammar-cell-word"), "{html}");
        assert!(html.contains("grammar-cell-ipa"), "{html}");
        assert!(html.contains("form") && !html.contains("form<"), "{html}");
        assert!(html.contains("[form&#38;]"), "{html}");
    }

    #[test]
    fn canonical_renderer_preserves_nested_headings_and_cell_merges() {
        let body = GrammarBody {
            columns: vec![GrammarColumn::Group {
                heading: "number".into(),
                autogenerated: false,
                columns: vec![
                    GrammarColumn::Individual {
                        heading: "singular".into(),
                        autogenerated: false,
                    },
                    GrammarColumn::Individual {
                        heading: "plural".into(),
                        autogenerated: false,
                    },
                ],
            }],
            rows: vec![GrammarRow::Group {
                heading: "case".into(),
                autogenerated: false,
                rows: vec![
                    GrammarRow::Individual {
                        heading: "nominative".into(),
                        autogenerated: false,
                        cells: vec![
                            GrammarCell {
                                changes: String::new(),
                                rowspan: 2,
                                colspan: 1,
                            },
                            GrammarCell::default(),
                        ],
                    },
                    GrammarRow::Individual {
                        heading: "accusative".into(),
                        autogenerated: false,
                        cells: vec![GrammarCell::default(), GrammarCell::default()],
                    },
                ],
            }],
        };
        let table = GrammarTable {
            id: uuid::Uuid::nil(),
            language_id: uuid::Uuid::nil(),
            name: "declension".into(),
            description: String::new(),
            preamble: String::new(),
            body: serde_json::to_value(body).unwrap(),
            position: 0,
            schema_version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: None,
            updated_by: None,
            word_class_ids: vec![],
            category_ids: vec![],
        };
        let rendered = RenderedGrammarTable {
            table_id: table.id,
            cells: vec![
                RenderedGrammarCell {
                    row: 0,
                    column: 0,
                    value: "one".into(),
                    ipa: None,
                    rowspan: 2,
                    colspan: 1,
                },
                RenderedGrammarCell {
                    row: 0,
                    column: 1,
                    value: "two".into(),
                    ipa: None,
                    rowspan: 1,
                    colspan: 1,
                },
                RenderedGrammarCell {
                    row: 1,
                    column: 1,
                    value: "three".into(),
                    ipa: None,
                    rowspan: 1,
                    colspan: 1,
                },
            ],
        };

        let html = render_html(&table, &rendered).unwrap();
        assert!(html.contains("<th colspan=\"2\">number</th>"), "{html}");
        assert!(html.contains("<th rowspan=\"2\">case</th>"), "{html}");
        assert!(
            html.contains("<td rowspan=\"2\"><span class=\"grammar-cell-word\">one"),
            "{html}"
        );
        assert_eq!(html.matches("<td").count(), 3, "{html}");
    }

    #[test]
    fn example_hint_links_to_the_word_and_escapes_its_definition() {
        let html = render_example_hint("example", &word(""), Some("to <move>"));
        assert!(html.contains("href=\"/languages/example/words/spelling/1\""));
        assert!(
            html.contains(">spelling</a> \"to &#60;move&#62;\" as an example"),
            "{html}"
        );
    }
}
