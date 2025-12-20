use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{query, FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::{err::{bad_request, AppError, AppResult}, model::{language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, users::User, words::Word}, pagination::{PaginatedRequest, PaginatedResponse}, util::{ensure_verified, repo_from_parts, AppState}};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "word_relation_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum WordRelationType {
    Derived,
    Descendant,
    Compound,
    Calque,
    Borrowed,
    Related,
    SeeAlso,
}

impl TryFrom<WordRelationType> for CognacyRelationKindV1 {
    type Error = AppError;

    fn try_from(value: WordRelationType) -> Result<Self, Self::Error> {
        match value {
            WordRelationType::Derived => Ok(CognacyRelationKindV1::Derived),
            WordRelationType::Descendant => Ok(CognacyRelationKindV1::Descendant),
            WordRelationType::Compound => Ok(CognacyRelationKindV1::Compound),
            WordRelationType::Calque => Ok(CognacyRelationKindV1::Calque),
            WordRelationType::Borrowed => Ok(CognacyRelationKindV1::Borrowed),
            _ => Err(bad_request("word relation type cannot be converted to cognacy relation kind")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WordRelation {
    pub id: Uuid,
    pub antecedent: Uuid,
    pub consequent: Uuid,
    pub kind: WordRelationType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DBCognacy {
    pub id: Uuid,
    pub tree: JsonValue,
    pub schema_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Cognacy {
    pub id: Uuid,
    pub inner: CognacyInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CognacyInner {
    V1(CognacySchemaV1),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognacySchemaV1 {
    pub edges: Vec<CognacyEdgeV1>,
    pub schema_version: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CognacyEdgeV1 {
    pub id: Uuid,
    pub antecedent: Uuid,
    pub consequent: Uuid,
    pub kind: CognacyRelationKindV1,
}

impl CognacySchemaV1 {
    // todo: wow this is not a great api
    /// Merge another cognacy schema into this one by adding the given edge.
    fn merge(mut self, other: Option<CognacySchemaV1>, edge: CognacyEdgeV1) -> AppResult<CognacySchemaV1> {

        // A cognacy is a DAG, so to merge two cognacies we need to ensure that adding the new edge does not create a cycle.
        // We can do this by performing a DFS from the consequent node and ensuring we do not reach the antecedent node.

        // look ma, Introduction to Algorithms, fourth edition by Cormen et al!
        fn dfs(
            adjacency_list: &HashMap<Uuid, Vec<Uuid>>,
            current: Uuid,
            target: Uuid,
            visited: &mut HashMap<Uuid, bool>,
        ) -> bool {
            if current == target {
                return true;
            }
            if let Some(&was_visited) = visited.get(&current) {
                if was_visited {
                    return false;
                }
            }
            visited.insert(current, true);
            if let Some(neighbors) = adjacency_list.get(&current) {
                for &neighbor in neighbors {
                    if dfs(adjacency_list, neighbor, target, visited) {
                        return true;
                    }
                }
            }
            false
        }


        let mut adjacency_list: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for e in &self.edges {
            adjacency_list.entry(e.antecedent).or_default().push(e.consequent);
        }
        if let Some(other) = other {
        for e in &other.edges {
            // if we need to bail later, we don't do anything destructive to
            // cognacy_a yet, so we can merge the edges as long as we have
            // the edge in cache anyways
            self.edges.push(*e);
            adjacency_list.entry(e.antecedent).or_default().push(e.consequent);
        }
    }

        self.edges.push(edge);
        adjacency_list.entry(edge.antecedent).or_default().push(edge.consequent);

        let mut visited = HashMap::new();
        if dfs(&adjacency_list, edge.consequent, edge.antecedent, &mut visited) {
            return Err(bad_request("adding this word relation would create a cycle in the cognacy graph"));
        }
        // graph is acyclic still!

        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CognacyRelationKindV1 {
    Derived,
    Descendant,
    Compound,
    Calque,
    Borrowed
}

pub struct WordRelationRepository {
    state: AppState,
}

#[derive(Debug, Clone)]
pub struct CreateWordRelation {
    pub antecedent: Word,
    pub consequent: Word,
    pub kind: WordRelationType
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[derive(strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum RelationDirection {
    Antecedent,
    Consequent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchWordRelations {
    pub kind: Option<WordRelationType>,
    pub direction: Option<RelationDirection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRelationSearchResult {
    pub kind: WordRelationType,
    pub related_word: Word,
    pub direction: RelationDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognacyFull {
    pub cognacy: Cognacy,
    pub words: HashMap<Uuid, Word>,
}


impl WordRelationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    // TODO: might need a `create_with_tx`
    pub async fn create(&self, requestor: &User, relation: CreateWordRelation) -> AppResult<WordRelation> {
        ensure_verified(requestor)?;

        let antecedent = relation.antecedent;
        let consequent = relation.consequent;
        
        let permissions = LanguagePermissionRepository::new(self.state.clone());

        if !permissions.has_permission(requestor.id, antecedent.language, PermissionLevel::Editor).await? {
            return Err(bad_request("you don't have permission to create word relations for this language"));
        }

        if !permissions.has_permission(requestor.id, consequent.language, PermissionLevel::Editor).await? {
            return Err(bad_request("you don't have permission to create word relations for this language"));
        }

        let mut tx = self.state.pool.begin().await?;

        let word_relation = sqlx::query_as!(WordRelation,
                r#"
                    INSERT INTO word_relations (antecedent, consequent, kind, created_by, updated_by)
                    VALUES ($1, $2, $3 :: word_relation_type, $4, $4)
                    RETURNING id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
                "#,
                antecedent.id,
                consequent.id,
                relation.kind as WordRelationType,
                requestor.id,
            ).fetch_one(&mut *tx).await?;

        // only update cognacy graph for relation types that are part of etymology
        // see_also and related don't affect the cognacy graph
        if let Ok(cognacy_relation_kind) = CognacyRelationKindV1::try_from(relation.kind) {
            let edge = CognacyEdgeV1 {
                id: word_relation.id,
                antecedent: antecedent.id,
                consequent: consequent.id,
                kind: cognacy_relation_kind,
            };

            let (antecedent_cognacy, consequent_cognacy) = tokio::try_join!(
                self.find_cognacy(&antecedent),
                self.find_cognacy(&consequent),
            )?;

            match (antecedent_cognacy, consequent_cognacy) {
                (Some(antecedent_cognacy), Some(consequent_cognacy)) => {
                    let antecedent_id = antecedent_cognacy.id;
                    let consequent_id = consequent_cognacy.id;
                    let CognacyInner::V1(antecedent_schema) = antecedent_cognacy.inner;
                    let CognacyInner::V1(consequent_schema) = consequent_cognacy.inner;
                    self.merge_v1(&mut tx, (antecedent_id, antecedent_schema), Some((consequent_id, consequent_schema)), edge).await?;
                }
                (Some(cognacy), None) | (None, Some(cognacy)) => {
                    let id = cognacy.id;
                    let CognacyInner::V1(schema) = cognacy.inner;
                    self.merge_v1(&mut tx, (id, schema), None, edge).await?;
                }
                (None, None) => {
                    // neither word is in a cognacy graph, create a new one
                    let edges = vec![edge];

                    let new_cognacy = CognacySchemaV1 {
                        edges,
                        schema_version: 1,
                    };

                    let cognacy_id = sqlx::query_scalar!(
                        r#"
                            INSERT INTO cognacies (tree, schema_version)
                            VALUES ($1, $2)
                            RETURNING id
                        "#,
                        serde_json::to_value(&new_cognacy).map_err(|e| bad_request(format!("failed to serialize cognacy schema: {e}")))?,
                        new_cognacy.schema_version,
                    ).fetch_one(&mut *tx)
                    .await?;

                    // update both words to point to the new cognacy
                    sqlx::query!(
                        r#"
                            UPDATE words
                            SET cognacy = $1
                            WHERE id = $2 OR id = $3
                        "#,
                        cognacy_id,
                        antecedent.id,
                        consequent.id,
                    ).execute(&mut *tx)
                    .await?;
                }
            }
        }

        tx.commit().await?;

        Ok(word_relation)
    }

    async fn find_cognacy(&self, word: &Word) -> AppResult<Option<Cognacy>> {
        if let Some(cognacy_id) = word.cognacy {
            let cognacy = sqlx::query_as::<_, DBCognacy>(
                "SELECT * FROM cognacies WHERE id = $1"
            )
            .bind(cognacy_id)
            .fetch_optional(&self.state.pool)
            .await?;

            if let Some(cognacy) = cognacy {
                match cognacy.schema_version {
                    1 => {
                        let schema: CognacySchemaV1 = serde_json::from_value(cognacy.tree)
                            .map_err(|e| bad_request(format!("failed to parse cognacy schema: {e}")))?;
                        Ok(Some(Cognacy {
                            id: cognacy.id,
                            inner: CognacyInner::V1(schema),
                        }))
                    }
                    _ => Err(bad_request("unsupported cognacy schema version")),
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn merge_v1(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cognacy_a: (Uuid, CognacySchemaV1),
        cognacy_b: Option<(Uuid, CognacySchemaV1)>,
        edge: CognacyEdgeV1,
    ) -> AppResult<Cognacy> {
        let (cognacy_a_id, cognacy_a_schema) = cognacy_a;

        // merge the schemas using the graph logic
        let merged_schema = cognacy_a_schema.merge(
            cognacy_b.as_ref().map(|(_, schema)| schema.clone()),
            edge,
        )?;

        // update cognacy_a with the merged schema
        sqlx::query!(
            r#"
                UPDATE cognacies
                SET tree = $1, schema_version = $2
                WHERE id = $3
            "#,
            serde_json::to_value(&merged_schema)
                .map_err(|e| bad_request(format!("failed to serialize cognacy schema: {e}")))?,
            merged_schema.schema_version,
            cognacy_a_id,
        )
        .execute(&mut **tx)
        .await?;

        // if there was a second cognacy, delete it and update all words that pointed to it
        if let Some((cognacy_b_id, _)) = cognacy_b {
            if cognacy_a_id != cognacy_b_id {
                // update all words that pointed to cognacy_b to point to cognacy_a
                sqlx::query!(
                    r#"
                        UPDATE words
                        SET cognacy = $1
                        WHERE cognacy = $2
                    "#,
                    cognacy_a_id,
                    cognacy_b_id,
                )
                .execute(&mut **tx)
                .await?;

                // delete the old cognacy
                sqlx::query!(
                    r#"
                        DELETE FROM cognacies
                        WHERE id = $1
                    "#,
                    cognacy_b_id,
                )
                .execute(&mut **tx)
                .await?;
            }
        }

        Ok(Cognacy {
            id: cognacy_a_id,
            inner: CognacyInner::V1(merged_schema),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub async fn delete(&self, requestor: &User, antecedent: &Word, consequent: &Word) -> AppResult<()> {
        ensure_verified(requestor)?;

        let permissions = LanguagePermissionRepository::new(self.state.clone());

        if !permissions.has_permission(requestor.id, antecedent.language, PermissionLevel::Editor).await? {
            return Err(bad_request("you don't have permission to delete word relations for this language"));
        }

        if !permissions.has_permission(requestor.id, consequent.language, PermissionLevel::Editor).await? {
            return Err(bad_request("you don't have permission to delete word relations for this language"));
        }

        let mut tx = self.state.pool.begin().await?;

        // find the relation between these two words (match exact direction)
        let relation = sqlx::query_as!(
            WordRelation,
            r#"
                DELETE FROM word_relations
                WHERE antecedent = $1 AND consequent = $2
                RETURNING id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
            "#,
            antecedent.id,
            consequent.id,
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(relation) = relation else {
            return Err(bad_request("no relation exists between these words"));
        };

        // if this was an etymological relation, we need to update the cognacy graph
        if CognacyRelationKindV1::try_from(relation.kind).is_ok() {
            // get the cognacy for the antecedent
            if let Some(cognacy) = self.find_cognacy(antecedent).await? {
                let CognacyInner::V1(mut schema) = cognacy.inner;

                // remove the edge from the schema
                schema.edges.retain(|e| e.id != relation.id);

                // check if the graph is still connected or if it split into components
                let components = Self::find_connected_components(&schema);

                if components.len() == 1 {
                    // graph is still connected, just update the cognacy
                    sqlx::query!(
                        r#"
                            UPDATE cognacies
                            SET tree = $1
                            WHERE id = $2
                        "#,
                        serde_json::to_value(&schema).map_err(|e| bad_request(format!("failed to serialize cognacy schema: {e}")))?,
                        cognacy.id,
                    )
                    .execute(&mut *tx)
                    .await?;
                } else if components.is_empty() {
                    // no edges left, delete the cognacy and clear word references
                    sqlx::query!(
                        r#"
                            DELETE FROM cognacies
                            WHERE id = $1
                        "#,
                        cognacy.id,
                    )
                    .execute(&mut *tx)
                    .await?;

                    sqlx::query!(
                        r#"
                            UPDATE words
                            SET cognacy = NULL
                            WHERE cognacy = $1
                        "#,
                        cognacy.id,
                    )
                    .execute(&mut *tx)
                    .await?;
                } else {
                    // graph split into multiple components, create new cognacies
                    for component_edges in components {
                        let new_schema = CognacySchemaV1 {
                            edges: component_edges.clone(),
                            schema_version: 1,
                        };

                        let new_cognacy_id = sqlx::query_scalar!(
                            r#"
                                INSERT INTO cognacies (tree, schema_version)
                                VALUES ($1, $2)
                                RETURNING id
                            "#,
                            serde_json::to_value(&new_schema).map_err(|e| bad_request(format!("failed to serialize cognacy schema: {e}")))?,
                            new_schema.schema_version,
                        )
                        .fetch_one(&mut *tx)
                        .await?;

                        // collect all word IDs in this component
                        let word_ids: Vec<Uuid> = component_edges.iter()
                            .flat_map(|e| vec![e.antecedent, e.consequent])
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();

                        // update words in this component to point to the new cognacy
                        sqlx::query!(
                            r#"
                                UPDATE words
                                SET cognacy = $1
                                WHERE id = ANY($2)
                            "#,
                            new_cognacy_id,
                            &word_ids,
                        )
                        .execute(&mut *tx)
                        .await?;
                    }

                    // delete the old cognacy
                    sqlx::query!(
                        r#"
                            DELETE FROM cognacies
                            WHERE id = $1
                        "#,
                        cognacy.id,
                    )
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(())
    }

    fn find_connected_components(schema: &CognacySchemaV1) -> Vec<Vec<CognacyEdgeV1>> {
        use std::collections::{HashMap, HashSet};

        if schema.edges.is_empty() {
            return vec![];
        }

        // build adjacency list (undirected graph for component finding)
        let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for edge in &schema.edges {
            adjacency.entry(edge.antecedent).or_default().push(edge.consequent);
            adjacency.entry(edge.consequent).or_default().push(edge.antecedent);
        }

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut components: Vec<Vec<CognacyEdgeV1>> = Vec::new();

        for edge in &schema.edges {
            if !visited.contains(&edge.antecedent) {
                // start a new component
                let mut component_nodes: HashSet<Uuid> = HashSet::new();
                let mut stack = vec![edge.antecedent];

                while let Some(node) = stack.pop() {
                    if visited.insert(node) {
                        component_nodes.insert(node);
                        if let Some(neighbors) = adjacency.get(&node) {
                            for &neighbor in neighbors {
                                if !visited.contains(&neighbor) {
                                    stack.push(neighbor);
                                }
                            }
                        }
                    }
                }

                // collect all edges in this component (both endpoints must be in the component)
                let component_edges: Vec<CognacyEdgeV1> = schema.edges.iter()
                    .filter(|e| component_nodes.contains(&e.antecedent) && component_nodes.contains(&e.consequent))
                    .copied()
                    .collect();

                components.push(component_edges);
            }
        }

        components
    }

    #[allow(clippy::too_many_lines)]
    pub async fn search(&self, pagination: PaginatedRequest, search: SearchWordRelations, word: &Word) -> AppResult<PaginatedResponse<WordRelationSearchResult>> {
        let items = query!(
            r#"
                SELECT DISTINCT ON (words.id)
                    words.id,
                    words.language,
                    words.word_class,
                    words.cognacy,
                    words.word,
                    words.slug,
                    words.lemma,
                    words.ipa,
                    words.notes,
                    words.extra,
                    words.like_count,
                    words.created_at,
                    words.updated_at,
                    words.created_by as "_created_by",
                    words.updated_by as "_updated_by",
                    bookmarks.slug as "bookmark: String",
                    languages.code as "language_code: Option<String>",
                    word_classes.abbreviation as "word_class_abbreviation: Option<String>",
                    created.username as "created_by: Option<String>",
                    updated.username as "updated_by: Option<String>",
                    word_relations.kind as "kind: WordRelationType",
                    (CASE
                        WHEN word_relations.antecedent = $2 THEN 'consequent'
                        ELSE 'antecedent'
                    END) as "direction!: String"
                FROM word_relations
                JOIN words ON
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.consequent = words.id
                        WHEN $1 = 'consequent' THEN word_relations.antecedent = words.id
                        ELSE word_relations.consequent = words.id OR word_relations.antecedent = words.id
                    END)
                JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                LEFT JOIN languages ON languages.id = words.language
                LEFT JOIN word_classes ON word_classes.id = words.word_class
                LEFT JOIN users AS created ON created.id = words.created_by
                LEFT JOIN users AS updated ON updated.id = words.updated_by
                WHERE
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.antecedent = $2
                        WHEN $1 = 'consequent' THEN word_relations.consequent = $2
                        ELSE word_relations.antecedent = $2 OR word_relations.consequent = $2
                    END)
                    AND ($3::word_relation_type IS NULL OR word_relations.kind = $3)
                    AND words.id <> $2
                ORDER BY words.id
                LIMIT $4 OFFSET $5
            "#,
            search.direction.map(|d| d.to_string()),
            word.id,
            search.kind as _,
            i64::from(pagination.limit),
            i64::from(pagination.offset),
        ).fetch_all(&self.state.pool);

        let count = sqlx::query_scalar!(
            r#"
                SELECT COUNT(DISTINCT words.id)
                FROM word_relations
                JOIN words ON
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.consequent = words.id
                        WHEN $1 = 'consequent' THEN word_relations.antecedent = words.id
                        ELSE word_relations.consequent = words.id OR word_relations.antecedent = words.id
                    END)
                WHERE
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.antecedent = $2
                        WHEN $1 = 'consequent' THEN word_relations.consequent = $2
                        ELSE word_relations.antecedent = $2 OR word_relations.consequent = $2
                    END)
                    AND ($3::word_relation_type IS NULL OR word_relations.kind = $3)
                    AND words.id <> $2
            "#,
            search.direction.map(|d| d.to_string()),
            word.id,
            search.kind as _,
        )
        .fetch_one(&self.state.pool);

        let (items, count) = tokio::try_join!(items, count)?;

        let items: Vec<_> = items.into_iter().map(|record| {
            let related_word = Word {
                id: record.id,
                language: record.language,
                word_class: record.word_class,
                cognacy: record.cognacy,
                word: record.word,
                slug: record.slug,
                lemma: record.lemma,
                ipa: record.ipa,
                notes: record.notes,
                extra: record.extra,
                like_count: record.like_count,
                created_at: record.created_at,
                updated_at: record.updated_at,
                _created_by: record._created_by,
                _updated_by: record._updated_by,
                bookmark: record.bookmark,
                language_code: record.language_code,
                word_class_abbreviation: record.word_class_abbreviation,
                created_by: record.created_by,
                updated_by: record.updated_by,
            };
            let direction = RelationDirection::from_str(&record.direction).unwrap();
            WordRelationSearchResult {
                kind: record.kind,
                related_word,
                direction,
            }
        }).collect();


        let total = count.unwrap_or(0);
        let has_more = (i64::from(pagination.offset) + i64::try_from(items.len()).unwrap_or(i64::MAX)) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }

    pub async fn get_cognacy(&self, word: &Word) -> AppResult<Option<CognacyFull>> {
        let cognacy = self.find_cognacy(word).await?;

        if let Some(cognacy) = cognacy {
            // get all word IDs from the cognacy graph
            let word_ids: Vec<Uuid> = match &cognacy.inner {
                CognacyInner::V1(schema) => {
                    let mut ids = std::collections::HashSet::new();
                    for edge in &schema.edges {
                        ids.insert(edge.antecedent);
                        ids.insert(edge.consequent);
                    }
                    ids.into_iter().collect()
                }
            };

            // fetch all words
            let words = sqlx::query_as!(
                Word,
                r#"
                    SELECT
                        words.id,
                        words.language,
                        words.word_class,
                        words.cognacy,
                        words.word,
                        words.slug,
                        words.lemma,
                        words.ipa,
                        words.notes,
                        words.extra,
                        words.like_count,
                        words.created_at,
                        words.updated_at,
                        words.created_by as "_created_by!",
                        words.updated_by as "_updated_by!",
                        bookmarks.slug as "bookmark!",
                        languages.code as language_code,
                        word_classes.abbreviation as word_class_abbreviation,
                        created.username as created_by,
                        updated.username as updated_by
                    FROM words
                    JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                    LEFT JOIN languages ON languages.id = words.language
                    LEFT JOIN word_classes ON word_classes.id = words.word_class
                    LEFT JOIN users AS created ON created.id = words.created_by
                    LEFT JOIN users AS updated ON updated.id = words.updated_by
                    WHERE words.id = ANY($1)
                "#,
                &word_ids
            )
            .fetch_all(&self.state.pool)
            .await?;

            let words_map: HashMap<Uuid, Word> = words.into_iter().map(|w| (w.id, w)).collect();

            Ok(Some(CognacyFull {
                cognacy,
                words: words_map,
            }))
        } else {
            Ok(None)
        }
    }
}

repo_from_parts!(WordRelationRepository);