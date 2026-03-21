use crate::AppState;
use crate::err::{AppResult, bad_request, forbidden};
use crate::model::language_invites::PermissionLevel;
use crate::model::language_permissions::LanguagePermissionRepository;
use crate::model::languages::Language;
use crate::model::user_activities::{ActivityType, UserActivityRepository};
use crate::model::user_bans::UserBanRepository;
use crate::model::users::User;
use crate::pagination::{PaginatedRequest, PaginatedResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::collections::VecDeque;
use std::fmt::Write;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phoneme {
    pub text: String,
    pub annotations: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub phonemes: Vec<Phoneme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Row {
    Group { heading: String, rows: Vec<Row> },
    Individual { heading: String, cells: Vec<Cell> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Column {
    Group {
        heading: String,
        columns: Vec<Column>,
    },
    Individual {
        heading: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    pub rows: Vec<Row>,
    pub columns: Vec<Column>,
    pub annotations: Vec<String>,
}

impl Body {
    pub fn validate(&self) -> AppResult<()> {
        // invariants:
        // - all rows must have the same number of cells
        // - the number of cells in each row must match the number of individual columns
        // - all phonemes must be non-empty strings
        // - all references to annotations must be valid indices into the annotations array

        fn count_individual_columns(columns: &[Column]) -> usize {
            let mut count = 0;
            for column in columns {
                match column {
                    Column::Group { columns, .. } => {
                        count += count_individual_columns(columns);
                    }
                    Column::Individual { .. } => {
                        count += 1;
                    }
                }
            }
            count
        }

        fn validate_num_cells_in_rows(rows: &[Row], num_columns: usize) -> AppResult<()> {
            for row in rows {
                match row {
                    Row::Group { rows, .. } => {
                        validate_num_cells_in_rows(rows, num_columns)?;
                    }
                    Row::Individual { cells, .. } => {
                        if cells.len() != num_columns {
                            return Err(bad_request(format!(
                                "Each row must have the same number of cells as there are individual columns. Expected {}, found {}.",
                                num_columns,
                                cells.len()
                            )));
                        }
                    }
                }
            }
            Ok(())
        }

        fn validate_phonemes_in_rows(rows: &[Row], annotations_len: usize) -> AppResult<()> {
            for row in rows {
                match row {
                    Row::Group { rows, .. } => {
                        validate_phonemes_in_rows(rows, annotations_len)?;
                    }
                    Row::Individual { cells, .. } => {
                        for cell in cells {
                            for phoneme in &cell.phonemes {
                                if phoneme.text.trim().is_empty() {
                                    return Err(bad_request(
                                        "Phoneme strings cannot be empty.".to_owned(),
                                    ));
                                }
                                for &annotation_index in &phoneme.annotations {
                                    if annotation_index as usize >= annotations_len {
                                        return Err(bad_request(format!(
                                            "Annotation index {} is out of bounds.",
                                            annotation_index
                                        )));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }

        let num_columns = count_individual_columns(&self.columns);
        validate_num_cells_in_rows(&self.rows, num_columns)?;
        validate_phonemes_in_rows(&self.rows, self.annotations.len())?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PhonologyTable {
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub language_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub position: i32,

    pub body: Value,
    pub schema_version: i32,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Column {
    fn count_leaves(&self) -> usize {
        match self {
            Column::Individual { .. } => 1,
            Column::Group { columns, .. } => columns.iter().map(|c| c.count_leaves()).sum(),
        }
    }

    fn max_depth(columns: &[Column], depth: usize) -> usize {
        let mut max = depth;
        for col in columns {
            if let Column::Group { columns, .. } = col {
                max = max.max(Column::max_depth(columns, depth + 1));
            }
        }
        max
    }
}

struct HeaderCell {
    heading: String,
    colspan: usize,
    rowspan: usize,
}

pub struct TableRenderOptions {
    pub standalone_link: Option<String>,
    pub edit_links: Option<(String, String, String)>, // (edit meta, edit body, delete)
}

impl PhonologyTable {
    pub fn to_html(&self, options: &TableRenderOptions) -> AppResult<String> {
        fn max_row_group_depth(rows: &[Row], current_depth: usize) -> usize {
            let mut max_depth = current_depth;
            for row in rows {
                if let Row::Group { rows, .. } = row {
                    let depth = max_row_group_depth(rows, current_depth + 1);
                    if depth > max_depth {
                        max_depth = depth;
                    }
                }
            }
            max_depth
        }

        fn count_individual_rows(rows: &[Row]) -> usize {
            let mut count = 0;
            for row in rows {
                match row {
                    Row::Group { rows, .. } => count += count_individual_rows(rows),
                    Row::Individual { .. } => count += 1,
                }
            }
            count
        }

        // pending_groups: group headings from ancestor Row::Groups that haven't been
        // emitted yet — they get attached to the first <tr> they contain
        fn write_rows(
            html: &mut String,
            rows: &[Row],
            pending_groups: &mut Vec<(String, usize)>, // (heading, rowspan)
            max_row_group_depth: usize,
            current_depth: usize,
        ) {
            for row in rows {
                match row {
                    Row::Group { heading, rows } => {
                        let rowspan = count_individual_rows(rows);
                        pending_groups.push((heading.clone(), rowspan));
                        write_rows(
                            html,
                            rows,
                            pending_groups,
                            max_row_group_depth,
                            current_depth + 1,
                        );
                    }
                    Row::Individual { heading, cells } => {
                        html.push_str("<tr>");

                        // emit any pending group headers (they attach to this first row)
                        for (group_heading, rowspan) in pending_groups.drain(..) {
                            write!(html, "<th").unwrap();
                            if rowspan > 1 {
                                write!(html, " rowspan=\"{rowspan}\"").unwrap();
                            }
                            write!(html, ">{group_heading}</th>").unwrap();
                        }

                        // individual row heading — colspan to fill remaining group columns
                        let colspan = max_row_group_depth - current_depth;
                        write!(html, "<th").unwrap();
                        if colspan > 1 {
                            write!(html, " colspan=\"{colspan}\"").unwrap();
                        }
                        write!(html, ">{heading}</th>").unwrap();

                        for cell in cells {
                            html.push_str("<td>");
                            for (i, phoneme) in cell.phonemes.iter().enumerate() {
                                if i > 0 {
                                    html.push_str(", ");
                                }
                                html.push_str("<span class=\"phoneme\">");
                                html.push_str(&phoneme.text);
                                html.push_str("</span>");
                                for &annotation_index in &phoneme.annotations {
                                    write!(html, "<sup><a href=\"#annotation-{annotation_index}\" class=\"annotation\">{annotation_index}</a></sup>").unwrap();
                                }
                            }
                            html.push_str("</td>");
                        }
                        html.push_str("</tr>");
                    }
                }
            }
        }

        let mut html = String::new();
        let body: Body = serde_json::from_value(self.body.clone())?;
        
        html.push_str("<div class=\"phonology-table-container\">");

        html.push_str("<div class=\"header-with-actions\">");
        write!(html, "<h2 id=\"table-{}\">{}</h2>", self.id, self.name)?;
        html.push_str("</h2><ul>");
        if let Some(standalone_link) = &options.standalone_link {
            html.push_str(&format!(
                "<li><a class=\"with-icon\" href=\"{}\">",
                standalone_link
            ));
            html.push_str(r#"
                    <svg class="icon" xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><!-- Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE --><path fill="currentColor" d="M9 18h11v-2.675H9zM4 8.675h3V6H4zm0 4.675h3v-2.675H4zM4 18h3v-2.675H4zm5-4.65h11v-2.675H9zm0-4.675h11V6H9zM4 20q-.825 0-1.412-.587T2 18V6q0-.825.588-1.412T4 4h16q.825 0 1.413.588T22 6v12q0 .825-.587 1.413T20 20z"/></svg>
                    <span>view table</span></a></li>"#);
        };
        if let Some((edit_meta_link, edit_body_link, delete_link)) = &options.edit_links {
            html.push_str(&format!(
                "<li><a class=\"with-icon\" href=\"{}\">",
                edit_meta_link
            ));
            html.push_str(r#"
                    <svg class="icon" xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><!-- Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE --><path fill="currentColor" d="M5 21q-.825 0-1.412-.587T3 19V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v14q0 .825-.587 1.413T19 21zm0-2h14V5H5zM7 9h10v2H7zm0 4h7v2H7z"/></svg>
                <span>edit table metadata</span></a></li>"#);
            html.push_str(&format!(
                "<li><a class=\"with-icon\" href=\"{}\">",
                edit_body_link
            ));
            html.push_str(r#"
                    <svg class="icon" xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><!-- Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE --><path fill="currentColor" d="M5 21q-.825 0-1.412-.587T3 19V5q0-.825.588-1.412T5 3h14q.825 0 1.413.588T21 5v14q0 .825-.587 1.413T19 21zm0-2h14V5H5zM7 9h10v2H7zm0 4h7v2H7z"/></svg>
                <span>edit table body</span></a></li>"#);

            html.push_str(&format!(
                "<li><a class=\"with-icon\" href=\"{}\">",
                delete_link
            ));
            html.push_str(r#"
                <svg class="icon" xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><!-- Icon from Material Symbols by Google - https://github.com/google/material-design-icons/blob/master/LICENSE --><path fill="currentColor" d="M7 21q-.825 0-1.412-.587T5 19V6H4V4h5V3h6v1h5v2h-1v13q0 .825-.587 1.413T17 21zM17 6H7v13h10zM9 17h2V8H9zm4 0h2V8h-2zM7 6v13z"/></svg>
                <span>delete table</span></a></li>"#);
        }
        html.push_str("</ul></div>");

        write!(html, "<table class=\"phonology-table\" aria-labelledby=\"table-{}\">", self.id)?;

        // --- thead: column headers ---
        let max_col_depth = Column::max_depth(&body.columns, 1);
        let max_row_depth = max_row_group_depth(&body.rows, 1);

        // BFS to build header rows
        let mut header_rows: Vec<Vec<HeaderCell>> = (0..max_col_depth).map(|_| Vec::new()).collect();
        let mut queue: VecDeque<(&Column, usize)> =
            body.columns.iter().map(|c| (c, 0)).collect();

        while let Some((col, depth)) = queue.pop_front() {
            match col {
                Column::Group { heading, columns } => {
                    header_rows[depth].push(HeaderCell {
                        heading: heading.clone(),
                        colspan: col.count_leaves(),
                        rowspan: 1,
                    });
                    for child in columns {
                        queue.push_back((child, depth + 1));
                    }
                }
                Column::Individual { heading } => {
                    header_rows[depth].push(HeaderCell {
                        heading: heading.clone(),
                        colspan: 1,
                        rowspan: max_col_depth - depth,
                    });
                }
            }
        }

        html.push_str("<thead>");
        for (i, row) in header_rows.iter().enumerate() {
            html.push_str("<tr>");
            // first header row gets an empty corner cell spanning all row-header columns
            if i == 0 {
                write!(
                    &mut html,
                    "<th colspan=\"{max_row_depth}\" rowspan=\"{max_col_depth}\"></th>"
                )
                .unwrap();
            }
            for cell in row {
                write!(&mut html, "<th").unwrap();
                if cell.colspan > 1 {
                    write!(&mut html, " colspan=\"{}\"", cell.colspan).unwrap();
                }
                if cell.rowspan > 1 {
                    write!(&mut html, " rowspan=\"{}\"", cell.rowspan).unwrap();
                }
                write!(&mut html, ">{}</th>", cell.heading).unwrap();
            }
            html.push_str("</tr>");
        }
        html.push_str("</thead>");

        // --- tbody: data rows ---
        html.push_str("<tbody>");
        let mut pending_groups: Vec<(String, usize)> = Vec::new();
        write_rows(&mut html, &body.rows, &mut pending_groups, max_row_depth + 1, 1);
        html.push_str("</tbody>");

        html.push_str("</table>");

        html.push_str("<ol class=\"annotations\">");
        for (i, annotation) in body.annotations.iter().enumerate() {
            write!(html, "<li id=\"annotation-{i}\">{annotation}</li>").unwrap();
        }
        html.push_str("</ol>");
        html.push_str("</div>");
        Ok(html)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CreatePhonologyTable {
    pub language_id: Uuid,
    pub name: String,
    pub description: Option<String>,

    pub body: Body,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UpdatePhonologyTable {
    pub name: Option<String>,
    pub description: Option<String>,

    pub body: Option<Body>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPhonologyTable {
    pub q: Option<String>,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
}

pub struct PhonologyTableRepository {
    state: AppState,
}

impl PhonologyTableRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        req: CreatePhonologyTable,
    ) -> AppResult<PhonologyTable> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;
        req.body.validate()?;

        let can_edit = LanguagePermissionRepository::new(self.state.clone())
            .has_permission(requestor.id, req.language_id, PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to create phonology tables for this language.",
            ));
        }

        let result = sqlx::query_as!(
            PhonologyTable,
            r#"
            insert into phonology_tables (language_id, name, description, body, position, schema_version)
            values ($1, $2, $3, $4,
            COALESCE((SELECT MAX(position) FROM phonology_tables WHERE language_id = $1), -1) + 1,
            1)
            returning *
            "#,
            req.language_id,
            req.name,
            req.description,
            serde_json::to_value(&req.body).unwrap(),
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(result)
    }

    pub async fn swap(
        &self,
        requestor: &User,
        language_id: Uuid,
        id1: Uuid,
        id2: Uuid,
    ) -> AppResult<()> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let can_edit = LanguagePermissionRepository::new(self.state.clone())
            .has_permission(requestor.id, language_id, PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to reorder phonology tables for this language.",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        let table1 = sqlx::query_as!(
            PhonologyTable,
            "select * from phonology_tables where id = $1 and language_id = $2",
            id1,
            language_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let table2 = sqlx::query_as!(
            PhonologyTable,
            "select * from phonology_tables where id = $1 and language_id = $2",
            id2,
            language_id
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            "update phonology_tables set position = $1 where id = $2",
            table2.position,
            table1.id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "update phonology_tables set position = $1 where id = $2",
            table1.position,
            table2.id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn update(
        &self,
        requestor: &User,
        id: Uuid,
        req: UpdatePhonologyTable,
    ) -> AppResult<PhonologyTable> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing = sqlx::query_as!(
            PhonologyTable,
            "select * from phonology_tables where id = $1",
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        let can_edit = LanguagePermissionRepository::new(self.state.clone())
            .has_permission(requestor.id, existing.language_id, PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to edit phonology tables for this language.",
            ));
        }

        if let Some(body) = &req.body {
            body.validate()?;
        }

        let updated = sqlx::query_as!(
            PhonologyTable,
            r#"
            update phonology_tables
            set name = COALESCE($1, name),
                description = COALESCE($2, description),
                body = COALESCE($3, body),
                schema_version = CASE WHEN $3 IS NOT NULL THEN schema_version + 1 ELSE schema_version END,
                updated_at = now()
            where id = $4
            returning *
            "#,
            req.name,
            req.description,
            req.body.as_ref().map(|b| serde_json::to_value(b).unwrap()),
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        UserActivityRepository::new(self.state.clone())
            .create(
                requestor.id,
                ActivityType::UpdateLanguage,
                existing.language_id,
                "language",
                None,
                None,
            )
            .await?;

        Ok(updated)
    }

    pub async fn delete(&self, requestor: &User, id: Uuid) -> AppResult<()> {
        UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing = sqlx::query_as!(
            PhonologyTable,
            "select * from phonology_tables where id = $1",
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        let can_edit = LanguagePermissionRepository::new(self.state.clone())
            .has_permission(requestor.id, existing.language_id, PermissionLevel::Editor)
            .await?;

        if !can_edit {
            return Err(forbidden(
                "You do not have permission to delete phonology tables for this language.",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        sqlx::query!("delete from phonology_tables where id = $1", id)
            .execute(&mut *tx)
            .await?;

        // decrement position of all tables with greater position

        sqlx::query!(
            "update phonology_tables set position = position - 1 where language_id = $1 and position > $2",
            existing.language_id,
            existing.position
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn get(&self, language: &Language, id: Uuid) -> AppResult<PhonologyTable> {
        let table = sqlx::query_as!(
            PhonologyTable,
            "select * from phonology_tables where id = $1 and language_id = $2",
            id,
            language.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(table)
    }

    pub async fn search(
        &self,
        language: &Language,
        pagination: PaginatedRequest,
        search: SearchPhonologyTable,
    ) -> AppResult<PaginatedResponse<PhonologyTable>> {
        let items_future = sqlx::query_as!(
            PhonologyTable,
            r#"
                SELECT pt.*
                FROM phonology_tables pt
                WHERE
                pt.language_id = $1
                AND ($5::TEXT IS NULL OR pt.created_at >= $5::TEXT::timestamptz)
                AND ($6::TEXT IS NULL OR pt.created_at <= $6::TEXT::timestamptz)
                ORDER BY (
                    CASE
                        WHEN $2::TEXT IS NOT NULL AND pt.name ILIKE '%' || $2 || '%' THEN 100.0
                        WHEN $2::TEXT IS NOT NULL AND pt.description ILIKE '%' || $2 || '%' THEN 80.0
                        ELSE 0.0
                    END +
                    CASE WHEN $2::TEXT IS NOT NULL THEN
                        similarity(pt.name, $2) * 3.0 +
                        COALESCE(similarity(pt.description, $2), 0.0) * 1.0
                    ELSE 0.0
                    END
                ) DESC, pt.position ASC, pt.id DESC
                LIMIT $3
                OFFSET $4
            "#,
            language.id,
            search.q,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
            search.created_after.map(|d| d.to_rfc3339()),
            search.created_before.map(|d| d.to_rfc3339()),
        )
        .fetch_all(&self.state.pool);

        let count_future = sqlx::query_scalar!(
            r#"
                SELECT COUNT(*)
                FROM phonology_tables pt
                WHERE pt.language_id = $1
                AND ($2::TEXT IS NULL OR pt.created_at >= $2::TEXT::timestamptz)
                AND ($3::TEXT IS NULL OR pt.created_at <= $3::TEXT::timestamptz)
            "#,
            language.id,
            search.created_after.map(|d| d.to_rfc3339()),
            search.created_before.map(|d| d.to_rfc3339()),
        )
        .fetch_one(&self.state.pool);

        let (items, total_count) = tokio::try_join!(items_future, count_future)?;

        let total = total_count.unwrap_or(0);
        let has_more =
            (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

crate::util::repo_from_parts!(PhonologyTableRepository);
