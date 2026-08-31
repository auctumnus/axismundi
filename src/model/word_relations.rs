use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, Postgres, Transaction, query};
use uuid::Uuid;

use crate::{
    err::{AppError, AppResult, bad_request},
    model::{
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository, users::User, words::Word,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, ensure_verified, repo_from_parts},
};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    sqlx::Type,
    strum::EnumString,
    strum::Display,
)]
#[sqlx(type_name = "word_relation_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
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
            _ => Err(bad_request(
                "word relation type cannot be converted to cognacy relation kind",
            )),
        }
    }
}

impl WordRelationType {
    pub fn text(&self, direction: &RelationDirection) -> &'static str {
        match (self, direction) {
            (WordRelationType::Derived, RelationDirection::Antecedent) => "derived into",
            (WordRelationType::Derived, RelationDirection::Consequent) => "derived from",
            (WordRelationType::Descendant, RelationDirection::Antecedent) => "descended into",
            (WordRelationType::Descendant, RelationDirection::Consequent) => "descended from",
            (WordRelationType::Compound, RelationDirection::Antecedent) => "compounded into",
            (WordRelationType::Compound, RelationDirection::Consequent) => "compounded from",
            (WordRelationType::Calque, RelationDirection::Antecedent) => "calqued into",
            (WordRelationType::Calque, RelationDirection::Consequent) => "calqued from",
            (WordRelationType::Borrowed, RelationDirection::Antecedent) => "borrowed into",
            (WordRelationType::Borrowed, RelationDirection::Consequent) => "borrowed from",
            (WordRelationType::Related, _) => "related to",
            (WordRelationType::SeeAlso, _) => "see also",
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
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
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
    fn merge(
        mut self,
        other: Option<CognacySchemaV1>,
        edge: CognacyEdgeV1,
    ) -> AppResult<CognacySchemaV1> {
        // A cognacy is a DAG, so to merge two cognacies we need to ensure that adding the new edge does not create a cycle.
        // We can do this by performing a DFS from the consequent node and ensuring we do not reach the antecedent node.

        let mut adjacency_list: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for e in &self.edges {
            adjacency_list
                .entry(e.antecedent)
                .or_default()
                .push(e.consequent);
        }
        if let Some(other) = other {
            for e in &other.edges {
                // if we need to bail later, we don't do anything destructive to
                // cognacy_a yet, so we can merge the edges as long as we have
                // the edge in cache anyways
                self.edges.push(*e);
                adjacency_list
                    .entry(e.antecedent)
                    .or_default()
                    .push(e.consequent);
            }
        }

        self.edges.push(edge);
        adjacency_list
            .entry(edge.antecedent)
            .or_default()
            .push(edge.consequent);

        let mut visited = HashMap::new();
        if crate::util::dfs(
            &adjacency_list,
            edge.consequent,
            edge.antecedent,
            &mut visited,
        ) {
            return Err(bad_request(
                "adding this word relation would create a cycle in the cognacy graph",
            ));
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
    Borrowed,
}

impl std::fmt::Display for CognacyRelationKindV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CognacyRelationKindV1::Derived => write!(f, "Derived"),
            CognacyRelationKindV1::Descendant => write!(f, "Descendant"),
            CognacyRelationKindV1::Compound => write!(f, "Compound"),
            CognacyRelationKindV1::Calque => write!(f, "Calque"),
            CognacyRelationKindV1::Borrowed => write!(f, "Borrowed"),
        }
    }
}

impl From<CognacyRelationKindV1> for WordRelationType {
    fn from(value: CognacyRelationKindV1) -> Self {
        match value {
            CognacyRelationKindV1::Derived => WordRelationType::Derived,
            CognacyRelationKindV1::Descendant => WordRelationType::Descendant,
            CognacyRelationKindV1::Compound => WordRelationType::Compound,
            CognacyRelationKindV1::Calque => WordRelationType::Calque,
            CognacyRelationKindV1::Borrowed => WordRelationType::Borrowed,
        }
    }
}

pub struct WordRelationRepository {
    state: AppState,
}

#[derive(Debug, Clone)]
pub struct CreateWordRelation {
    pub antecedent: Uuid,
    pub consequent: Uuid,
    pub kind: WordRelationType,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum RelationDirection {
    Antecedent,
    Consequent,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchWordRelations {
    pub q: Option<String>,
    pub kind: Option<WordRelationType>,
    pub direction: Option<RelationDirection>,
    pub non_cognacy_relations_only: Option<bool>,
}

crate::util::text_query!(SearchWordRelations);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRelationSearchResult {
    pub word: Word,
    pub language: String,
    pub language_code: String,
    pub into_other_language: bool,
    pub relation: WordRelationForDisplay,
    pub direction: RelationDirection,
    pub creator: User,
    pub created_at: DateTime<Utc>,
    pub preview_definitions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRelationForDisplay {
    pub kind: WordRelationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognacyFull {
    pub cognacy: Cognacy,
    pub words: HashMap<Uuid, Word>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeveledCognacy {
    pub levels: Vec<Vec<Uuid>>, // word IDs grouped by level
    pub edges: Vec<CognacyEdgeV1>,
    pub words: HashMap<Uuid, Word>,
}

impl WordRelationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(
        &self,
        requestor: &User,
        relation: CreateWordRelation,
    ) -> AppResult<WordRelation> {
        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let mut tx = self.state.pool.begin().await?;
        let word_relation = self.create_with_tx(requestor, relation, &mut tx).await?;
        tx.commit().await?;

        Ok(word_relation)
    }

    pub async fn create_with_tx(
        &self,
        requestor: &User,
        relation: CreateWordRelation,
        tx: &mut Transaction<'_, Postgres>,
    ) -> AppResult<WordRelation> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::CheckPermissionReq;

        let antecedent = relation.antecedent;
        let consequent = relation.consequent;

        let ante_word = sqlx::query!(
            "SELECT language, cognacy FROM words WHERE id = $1",
            antecedent,
        )
        .fetch_one(&mut **tx)
        .await?;

        let cons_word = sqlx::query!(
            "SELECT language, cognacy FROM words WHERE id = $1",
            consequent,
        )
        .fetch_one(&mut **tx)
        .await?;

        let word_relation = sqlx::query_as!(WordRelation,
                r#"
                    INSERT INTO word_relations (antecedent, consequent, kind, created_by, updated_by)
                    VALUES ($1, $2, $3 :: word_relation_type, $4, $4)
                    RETURNING id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
                "#,
                antecedent,
                consequent,
                relation.kind as WordRelationType,
                requestor.id,
            ).fetch_one(&mut **tx).await.map_err(|e| {
                if let sqlx::Error::Database(db_err) = &e {
                    if db_err.constraint() == Some("word_relations_antecedent_consequent_unique") {
                        return bad_request("a relation between these two words already exists");
                    }
                }
                e.into()
            })?;

        // Check permissions on both languages with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());

        let ante_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: ante_word.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Created,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: word_relation.id,
                    context: Some(serde_json::json!({
                        "role": "antecedent",
                        "word_id": antecedent,
                        "language_id": ante_word.language,
                    })),
                },
                tx,
            )
            .await?;

        let cons_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: cons_word.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Created,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: word_relation.id,
                    context: Some(serde_json::json!({
                        "role": "consequent",
                        "word_id": consequent,
                        "language_id": cons_word.language,
                    })),
                },
                tx,
            )
            .await?;

        if ante_perm == PermissionCheck::NoPermission || cons_perm == PermissionCheck::NoPermission
        {
            return Err(bad_request(
                "you don't have permission to create word relations for this language",
            ));
        }

        // only update cognacy graph for relation types that are part of etymology
        // see_also and related don't affect the cognacy graph
        if let Ok(cognacy_relation_kind) = CognacyRelationKindV1::try_from(relation.kind) {
            let edge = CognacyEdgeV1 {
                id: word_relation.id,
                antecedent,
                consequent,
                kind: cognacy_relation_kind,
            };

            let (antecedent_cognacy, consequent_cognacy) = tokio::try_join!(
                self.find_cognacy(ante_word.cognacy),
                self.find_cognacy(cons_word.cognacy),
            )?;

            match (antecedent_cognacy, consequent_cognacy) {
                (Some(antecedent_cognacy), Some(consequent_cognacy))
                    if antecedent_cognacy.id == consequent_cognacy.id =>
                {
                    // Both words already live in the same cognacy graph (e.g. a new
                    // edge between two nodes connected through a transitive path).
                    // Treat as a single cognacy so merge() doesn't fold the schema
                    // into itself and double every edge.
                    let id = antecedent_cognacy.id;
                    let CognacyInner::V1(schema) = antecedent_cognacy.inner;
                    self.merge_v1(tx, (id, schema), None, edge).await?;
                }
                (Some(antecedent_cognacy), Some(consequent_cognacy)) => {
                    let antecedent_id = antecedent_cognacy.id;
                    let consequent_id = consequent_cognacy.id;
                    let CognacyInner::V1(antecedent_schema) = antecedent_cognacy.inner;
                    let CognacyInner::V1(consequent_schema) = consequent_cognacy.inner;
                    self.merge_v1(
                        tx,
                        (antecedent_id, antecedent_schema),
                        Some((consequent_id, consequent_schema)),
                        edge,
                    )
                    .await?;
                }
                (Some(cognacy), None) | (None, Some(cognacy)) => {
                    let id = cognacy.id;
                    let CognacyInner::V1(schema) = cognacy.inner;
                    self.merge_v1(tx, (id, schema), None, edge).await?;

                    // update both words to point to the cognacy
                    sqlx::query!(
                        r#"
                            UPDATE words
                            SET cognacy = $1
                            WHERE id = $2 OR id = $3
                        "#,
                        id,
                        antecedent,
                        consequent,
                    )
                    .execute(&mut **tx)
                    .await?;
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
                        serde_json::to_value(&new_cognacy).map_err(|e| bad_request(format!(
                            "failed to serialize cognacy schema: {e}"
                        )))?,
                        new_cognacy.schema_version,
                    )
                    .fetch_one(&mut **tx)
                    .await?;

                    // update both words to point to the new cognacy
                    sqlx::query!(
                        r#"
                            UPDATE words
                            SET cognacy = $1
                            WHERE id = $2 OR id = $3
                        "#,
                        cognacy_id,
                        antecedent,
                        consequent,
                    )
                    .execute(&mut **tx)
                    .await?;
                }
            }
        }

        Ok(word_relation)
    }

    async fn find_cognacy(&self, cognacy_id: Option<Uuid>) -> AppResult<Option<Cognacy>> {
        if let Some(cognacy_id) = cognacy_id {
            let cognacy = sqlx::query_as::<_, DBCognacy>("SELECT * FROM cognacies WHERE id = $1")
                .bind(cognacy_id)
                .fetch_optional(&self.state.pool)
                .await?;

            if let Some(cognacy) = cognacy {
                match cognacy.schema_version {
                    1 => {
                        let schema: CognacySchemaV1 = serde_json::from_value(cognacy.tree)
                            .map_err(|e| {
                                bad_request(format!("failed to parse cognacy schema: {e}"))
                            })?;
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
        let merged_schema =
            cognacy_a_schema.merge(cognacy_b.as_ref().map(|(_, schema)| schema.clone()), edge)?;

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
    pub async fn delete(
        &self,
        requestor: &User,
        antecedent: &Word,
        consequent: &Word,
    ) -> AppResult<()> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::CheckPermissionReq;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

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

        // Check permissions on both languages with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());

        let ante_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: antecedent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: relation.id,
                    context: Some(serde_json::json!({
                        "role": "antecedent",
                        "word_id": antecedent.id,
                        "language_id": antecedent.language,
                    })),
                },
                &mut tx,
            )
            .await?;

        let cons_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: consequent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Deleted,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: relation.id,
                    context: Some(serde_json::json!({
                        "role": "consequent",
                        "word_id": consequent.id,
                        "language_id": consequent.language,
                    })),
                },
                &mut tx,
            )
            .await?;

        if ante_perm == PermissionCheck::NoPermission || cons_perm == PermissionCheck::NoPermission
        {
            return Err(bad_request(
                "you don't have permission to delete word relations for this language",
            ));
        }

        // if this was an etymological relation, we need to update the cognacy graph
        if CognacyRelationKindV1::try_from(relation.kind).is_ok() {
            if let Some(cognacy) = self.find_cognacy(antecedent.cognacy).await? {
                let CognacyInner::V1(mut schema) = cognacy.inner;
                schema.edges.retain(|e| e.id != relation.id);
                Self::persist_cognacy_after_edit(&mut tx, cognacy.id, schema).await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// Remove every cognacy edge referencing `word_id` from the cognacy graph and
    /// persist the resulting schema. Splits, shrinks, or deletes the cognacy as
    /// needed. Called when a word is deleted, since the `word_relations` FK
    /// cascade does not touch the cognacy json tree.
    pub async fn remove_word_from_cognacy(
        tx: &mut Transaction<'_, Postgres>,
        word_id: Uuid,
        cognacy_id: Uuid,
    ) -> AppResult<()> {
        let cognacy = sqlx::query_as::<_, DBCognacy>("SELECT * FROM cognacies WHERE id = $1")
            .bind(cognacy_id)
            .fetch_optional(&mut **tx)
            .await?;

        let Some(cognacy) = cognacy else {
            return Ok(());
        };

        if cognacy.schema_version != 1 {
            return Err(bad_request("unsupported cognacy schema version"));
        }

        let mut schema: CognacySchemaV1 = serde_json::from_value(cognacy.tree)
            .map_err(|e| bad_request(format!("failed to parse cognacy schema: {e}")))?;

        schema
            .edges
            .retain(|e| e.antecedent != word_id && e.consequent != word_id);

        Self::persist_cognacy_after_edit(tx, cognacy.id, schema).await
    }

    /// Persist a cognacy schema after some edges have been removed. Handles three
    /// cases: graph remains connected (update), graph empty (delete), graph split
    /// into multiple components (create new cognacies, delete original). In every
    /// case, words no longer in any edge get their `cognacy` pointer cleared.
    async fn persist_cognacy_after_edit(
        tx: &mut Transaction<'_, Postgres>,
        cognacy_id: Uuid,
        schema: CognacySchemaV1,
    ) -> AppResult<()> {
        let components = Self::find_connected_components(&schema);

        if components.is_empty() {
            sqlx::query!("DELETE FROM cognacies WHERE id = $1", cognacy_id)
                .execute(&mut **tx)
                .await?;

            sqlx::query!(
                "UPDATE words SET cognacy = NULL WHERE cognacy = $1",
                cognacy_id,
            )
            .execute(&mut **tx)
            .await?;
        } else if components.len() == 1 {
            sqlx::query!(
                "UPDATE cognacies SET tree = $1 WHERE id = $2",
                serde_json::to_value(&schema)
                    .map_err(|e| bad_request(format!("failed to serialize cognacy schema: {e}")))?,
                cognacy_id,
            )
            .execute(&mut **tx)
            .await?;

            let word_ids_in_graph: Vec<Uuid> = schema
                .edges
                .iter()
                .flat_map(|e| [e.antecedent, e.consequent])
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            sqlx::query!(
                "UPDATE words SET cognacy = NULL WHERE cognacy = $1 AND id <> ALL($2)",
                cognacy_id,
                &word_ids_in_graph,
            )
            .execute(&mut **tx)
            .await?;
        } else {
            for component_edges in components {
                let new_schema = CognacySchemaV1 {
                    edges: component_edges.clone(),
                    schema_version: 1,
                };

                let new_cognacy_id = sqlx::query_scalar!(
                    "INSERT INTO cognacies (tree, schema_version) VALUES ($1, $2) RETURNING id",
                    serde_json::to_value(&new_schema).map_err(|e| bad_request(format!(
                        "failed to serialize cognacy schema: {e}"
                    )))?,
                    new_schema.schema_version,
                )
                .fetch_one(&mut **tx)
                .await?;

                let word_ids: Vec<Uuid> = component_edges
                    .iter()
                    .flat_map(|e| [e.antecedent, e.consequent])
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                sqlx::query!(
                    "UPDATE words SET cognacy = $1 WHERE id = ANY($2)",
                    new_cognacy_id,
                    &word_ids,
                )
                .execute(&mut **tx)
                .await?;
            }

            sqlx::query!("DELETE FROM cognacies WHERE id = $1", cognacy_id)
                .execute(&mut **tx)
                .await?;
        }

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
            adjacency
                .entry(edge.antecedent)
                .or_default()
                .push(edge.consequent);
            adjacency
                .entry(edge.consequent)
                .or_default()
                .push(edge.antecedent);
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
                let component_edges: Vec<CognacyEdgeV1> = schema
                    .edges
                    .iter()
                    .filter(|e| {
                        component_nodes.contains(&e.antecedent)
                            && component_nodes.contains(&e.consequent)
                    })
                    .copied()
                    .collect();

                components.push(component_edges);
            }
        }

        components
    }

    pub async fn find_relation(
        &self,
        antecedent: &Word,
        consequent: &Word,
    ) -> AppResult<WordRelation> {
        let relation = sqlx::query_as!(
            WordRelation,
            r#"
                SELECT id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
                FROM word_relations
                WHERE antecedent = $1 AND consequent = $2
            "#,
            antecedent.id,
            consequent.id,
        )
        .fetch_optional(&self.state.pool)
        .await?;

        relation.ok_or_else(|| bad_request("no relation exists between these words"))
    }

    pub async fn update(
        &self,
        requestor: &User,
        antecedent: &Word,
        consequent: &Word,
        new_kind: WordRelationType,
    ) -> AppResult<WordRelation> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::CheckPermissionReq;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        // Get the existing relation to check if we can update it
        let existing_relation = self.find_relation(antecedent, consequent).await?;

        // Check if both old and new types affect cognacy
        let old_affects_cognacy = CognacyRelationKindV1::try_from(existing_relation.kind).is_ok();
        let new_affects_cognacy = CognacyRelationKindV1::try_from(new_kind).is_ok();

        // If cognacy status changes, we need to rebuild the cognacy graph, which is complex
        // For now, just handle the simple case where both affect cognacy or neither does
        if old_affects_cognacy != new_affects_cognacy {
            return Err(bad_request(
                "Cannot change relation type between etymological and non-etymological. Please delete and recreate the relation.",
            ));
        }

        let mut tx = self.state.pool.begin().await?;

        // Check permissions on both languages with audit
        let permissions = LanguagePermissionRepository::new(self.state.clone());

        let ante_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: antecedent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: existing_relation.id,
                    context: Some(serde_json::json!({
                        "role": "antecedent",
                        "word_id": antecedent.id,
                        "language_id": antecedent.language,
                        "new_kind": format!("{:?}", new_kind),
                    })),
                },
                &mut tx,
            )
            .await?;

        let cons_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: consequent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: existing_relation.id,
                    context: Some(serde_json::json!({
                        "role": "consequent",
                        "word_id": consequent.id,
                        "language_id": consequent.language,
                        "new_kind": format!("{:?}", new_kind),
                    })),
                },
                &mut tx,
            )
            .await?;

        if ante_perm == PermissionCheck::NoPermission || cons_perm == PermissionCheck::NoPermission
        {
            return Err(bad_request(
                "you don't have permission to edit word relations for this language",
            ));
        }

        // Simple case: just update the kind field
        let updated_relation = sqlx::query_as!(
            WordRelation,
            r#"
                UPDATE word_relations
                SET kind = $1 :: word_relation_type, updated_by = $2, updated_at = CURRENT_TIMESTAMP
                WHERE antecedent = $3 AND consequent = $4
                RETURNING id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
            "#,
            new_kind as WordRelationType,
            requestor.id,
            antecedent.id,
            consequent.id,
        )
        .fetch_one(&mut *tx)
        .await?;

        // If both types affect cognacy, we need to update the cognacy graph edge
        if old_affects_cognacy && new_affects_cognacy {
            if let Some(cognacy) = self.find_cognacy(antecedent.cognacy).await? {
                let new_cognacy_kind = CognacyRelationKindV1::try_from(new_kind)?;
                let CognacyInner::V1(mut schema) = cognacy.inner;

                // Find and update the edge in the cognacy graph
                for edge in &mut schema.edges {
                    if edge.id == updated_relation.id {
                        edge.kind = new_cognacy_kind;
                        break;
                    }
                }

                // Update the cognacy in the database
                sqlx::query!(
                    r#"
                        UPDATE cognacies
                        SET tree = $1, schema_version = $2
                        WHERE id = $3
                    "#,
                    serde_json::to_value(&schema).map_err(|e| bad_request(format!(
                        "failed to serialize cognacy schema: {e}"
                    )))?,
                    schema.schema_version,
                    cognacy.id,
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        Ok(updated_relation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn flip(
        &self,
        requestor: &User,
        antecedent: &Word,
        consequent: &Word,
    ) -> AppResult<WordRelation> {
        use crate::model::audit_log::{AuditActionType, AuditableResource, PermissionCheck};
        use crate::model::language_permissions::CheckPermissionReq;

        ensure_verified(requestor)?;

        crate::model::user_bans::UserBanRepository::new(self.state.clone())
            .ensure_not_banned(requestor.id)
            .await?;

        let existing_relation = self.find_relation(antecedent, consequent).await?;

        let mut tx = self.state.pool.begin().await?;

        let permissions = LanguagePermissionRepository::new(self.state.clone());

        let ante_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: antecedent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: existing_relation.id,
                    context: Some(serde_json::json!({
                        "role": "antecedent",
                        "word_id": antecedent.id,
                        "language_id": antecedent.language,
                        "flipped": true,
                    })),
                },
                &mut tx,
            )
            .await?;

        let cons_perm = permissions
            .check_permission_with_audit(
                CheckPermissionReq {
                    user: requestor.id,
                    language: consequent.language,
                    required_level: PermissionLevel::Editor,
                    action_type: AuditActionType::Updated,
                    resource_type: AuditableResource::WordRelation,
                    resource_id: existing_relation.id,
                    context: Some(serde_json::json!({
                        "role": "consequent",
                        "word_id": consequent.id,
                        "language_id": consequent.language,
                        "flipped": true,
                    })),
                },
                &mut tx,
            )
            .await?;

        if ante_perm == PermissionCheck::NoPermission || cons_perm == PermissionCheck::NoPermission
        {
            return Err(bad_request(
                "you don't have permission to edit word relations for this language",
            ));
        }

        if let Ok(cognacy_kind) = CognacyRelationKindV1::try_from(existing_relation.kind) {
            if let Some(cognacy) = self.find_cognacy(antecedent.cognacy).await? {
                let CognacyInner::V1(mut schema) = cognacy.inner;

                schema.edges.retain(|e| e.id != existing_relation.id);

                // ensure the flipped edge (consequent -> antecedent) doesn't introduce
                // a cycle: starting from the new edge's consequent (antecedent.id) we
                // must not be able to reach the new edge's antecedent (consequent.id).
                let mut adjacency_list: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
                for e in &schema.edges {
                    adjacency_list
                        .entry(e.antecedent)
                        .or_default()
                        .push(e.consequent);
                }
                adjacency_list
                    .entry(consequent.id)
                    .or_default()
                    .push(antecedent.id);

                let mut visited = HashMap::new();
                if crate::util::dfs(
                    &adjacency_list,
                    antecedent.id,
                    consequent.id,
                    &mut visited,
                ) {
                    return Err(bad_request(
                        "flipping this relation would create a cycle in the cognacy graph",
                    ));
                }

                schema.edges.push(CognacyEdgeV1 {
                    id: existing_relation.id,
                    antecedent: consequent.id,
                    consequent: antecedent.id,
                    kind: cognacy_kind,
                });

                sqlx::query!(
                    r#"
                        UPDATE cognacies
                        SET tree = $1
                        WHERE id = $2
                    "#,
                    serde_json::to_value(&schema).map_err(|e| bad_request(format!(
                        "failed to serialize cognacy schema: {e}"
                    )))?,
                    cognacy.id,
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        let updated_relation = sqlx::query_as!(
            WordRelation,
            r#"
                UPDATE word_relations
                SET antecedent = $2, consequent = $1, updated_by = $3, updated_at = CURRENT_TIMESTAMP
                WHERE antecedent = $1 AND consequent = $2
                RETURNING id, antecedent, consequent, kind as "kind: WordRelationType", created_at, updated_at, created_by, updated_by
            "#,
            antecedent.id,
            consequent.id,
            requestor.id,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("word_relations_antecedent_consequent_unique") {
                    return bad_request("a relation in the flipped direction already exists");
                }
            }
            e.into()
        })?;

        tx.commit().await?;

        Ok(updated_relation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn search(
        &self,
        pagination: PaginatedRequest,
        search: SearchWordRelations,
        word: &Word,
    ) -> AppResult<PaginatedResponse<WordRelationSearchResult>> {
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
                    languages.code as "word_language_code!: String",
                    languages.name as "word_language_name!: String",
                    word_classes.abbreviation as "word_class_abbreviation: Option<String>",
                    word_created.username as "word_created_by: Option<String>",
                    word_updated.username as "word_updated_by: Option<String>",
                    word_relations.kind as "kind!: WordRelationType",
                    word_relations.created_at as "relation_created_at!",
                    word_relations.created_by as "relation_created_by!",
                    relation_creator.id as "creator_id!",
                    relation_creator.username as "creator_username!",
                    relation_creator.display_name as "creator_display_name",
                    relation_creator.email as "creator_email!",
                    relation_creator.password_hash as "creator_password_hash!",
                    relation_creator.verified_at as "creator_verified_at",
                    relation_creator.description as "creator_description",
                    relation_creator.pronouns as "creator_pronouns",
                    relation_creator.gender as "creator_gender",
                    relation_creator.profile_picture_object_id as "creator_profile_picture_object_id",
                    relation_creator.tags as "creator_tags!",
                    creator_bookmarks.slug as "creator_bookmark!",
                    relation_creator.created_at as "creator_created_at!",
                    relation_creator.updated_at as "creator_updated_at!",
                    (CASE
                        WHEN word_relations.antecedent = $2 THEN 'consequent'
                        ELSE 'antecedent'
                    END) as "direction!: String",
                    COALESCE(preview_defs.definitions, ARRAY[]::text[]) as "preview_definitions!: Vec<String>"
                FROM word_relations
                JOIN words ON
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.consequent = words.id
                        WHEN $1 = 'consequent' THEN word_relations.antecedent = words.id
                        ELSE word_relations.consequent = words.id OR word_relations.antecedent = words.id
                    END)
                JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
                JOIN languages ON languages.id = words.language
                JOIN users AS relation_creator ON relation_creator.id = word_relations.created_by
                LEFT JOIN bookmarks AS creator_bookmarks ON creator_bookmarks.item = relation_creator.id AND creator_bookmarks.resource = 'user'
                LEFT JOIN word_classes ON word_classes.id = words.word_class
                LEFT JOIN users AS word_created ON word_created.id = words.created_by
                LEFT JOIN users AS word_updated ON word_updated.id = words.updated_by
                LEFT JOIN LATERAL (
                    SELECT array_agg(d.definition ORDER BY d.position) AS definitions
                    FROM (
                        SELECT definition, position
                        FROM definitions
                        WHERE definitions.word = words.id
                        ORDER BY position ASC
                        LIMIT 5
                    ) AS d
                ) AS preview_defs ON true
                WHERE
                    (CASE
                        WHEN $1 = 'antecedent' THEN word_relations.antecedent = $2
                        WHEN $1 = 'consequent' THEN word_relations.consequent = $2
                        ELSE word_relations.antecedent = $2 OR word_relations.consequent = $2
                    END)
                    AND (
                        CASE
                            WHEN $3 THEN word_relations.kind IN ('related', 'see_also')
                            ELSE $4::word_relation_type IS NULL OR word_relations.kind = $4
                        END
                    )
                    AND words.id <> $2
                ORDER BY words.id
                LIMIT $5 OFFSET $6
            "#,
            search.direction.map(|d| d.to_string()),
            word.id,
            search.non_cognacy_relations_only.unwrap_or(false),
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
                    AND (
                        CASE
                            WHEN $3 THEN word_relations.kind IN ('related', 'see_also')
                            ELSE $4::word_relation_type IS NULL OR word_relations.kind = $4
                        END
                    )
                    AND words.id <> $2
            "#,
            search.direction.map(|d| d.to_string()),
            word.id,
            search.non_cognacy_relations_only.unwrap_or(false),
            search.kind as _,
        )
        .fetch_one(&self.state.pool);

        let (items, count): (Vec<_>, Option<i64>) = tokio::try_join!(items, count)?;

        let items: Vec<_> = items
            .into_iter()
            .map(|record| {
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
                    language_code: Some(record.word_language_code.clone()),
                    language_name: Some(record.word_language_name.clone()),
                    word_class_abbreviation: record.word_class_abbreviation,
                    created_by: record.word_created_by,
                    updated_by: record.word_updated_by,
                };

                let creator = User {
                    id: record.creator_id,
                    username: record.creator_username,
                    display_name: record.creator_display_name,
                    email: record.creator_email,
                    password_hash: record.creator_password_hash,
                    verified_at: record.creator_verified_at,
                    description: record.creator_description,
                    pronouns: record.creator_pronouns,
                    gender: record.creator_gender,
                    profile_picture_object_id: record.creator_profile_picture_object_id,
                    banner_object_id: String::new(),
                    tags: record.creator_tags,
                    bookmark: record.creator_bookmark,
                    created_at: record.creator_created_at,
                    updated_at: record.creator_updated_at,
                };

                let direction = RelationDirection::from_str(&record.direction).unwrap();
                let into_other_language = related_word.language != word.language;

                WordRelationSearchResult {
                    word: related_word,
                    language: record.word_language_code.clone(),
                    language_code: record.word_language_code,
                    into_other_language,
                    relation: WordRelationForDisplay { kind: record.kind },
                    direction,
                    creator,
                    created_at: record.relation_created_at,
                    preview_definitions: record.preview_definitions,
                }
            })
            .collect();

        let total = count.unwrap_or(0);
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

    /// Filter cognacy edges to those relevant to `focus`, applying a one-hop cutoff:
    /// include all ancestors and descendants of `focus`, plus any node connected by
    /// a single edge to one of those (a co-ancestor or co-descendant), but do not
    /// follow further edges out of those one-hop nodes. Without this, a shared
    /// ancestor pulls in unrelated branches that a reader doesn't intuitively
    /// consider "related" to the focus word.
    fn filter_edges_for_focus(edges: &[CognacyEdgeV1], focus: Uuid) -> Vec<CognacyEdgeV1> {
        use std::collections::HashSet;

        let mut core: HashSet<Uuid> = HashSet::new();
        core.insert(focus);

        let mut to_visit = vec![focus];
        while let Some(node) = to_visit.pop() {
            for edge in edges {
                if edge.consequent == node && core.insert(edge.antecedent) {
                    to_visit.push(edge.antecedent);
                }
            }
        }

        let mut to_visit = vec![focus];
        while let Some(node) = to_visit.pop() {
            for edge in edges {
                if edge.antecedent == node && core.insert(edge.consequent) {
                    to_visit.push(edge.consequent);
                }
            }
        }

        edges
            .iter()
            .filter(|e| core.contains(&e.antecedent) || core.contains(&e.consequent))
            .copied()
            .collect()
    }

    pub async fn get_cognacy(&self, word: &Word) -> AppResult<Option<CognacyFull>> {
        let cognacy = self.find_cognacy(word.cognacy).await?;

        if let Some(cognacy) = cognacy {
            let (filtered_inner, word_ids): (CognacyInner, Vec<Uuid>) = match cognacy.inner {
                CognacyInner::V1(schema) => {
                    let filtered_edges = Self::filter_edges_for_focus(&schema.edges, word.id);
                    let mut ids = std::collections::HashSet::new();
                    ids.insert(word.id);
                    for edge in &filtered_edges {
                        ids.insert(edge.antecedent);
                        ids.insert(edge.consequent);
                    }
                    let new_schema = CognacySchemaV1 {
                        edges: filtered_edges,
                        schema_version: schema.schema_version,
                    };
                    (CognacyInner::V1(new_schema), ids.into_iter().collect())
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
                        COALESCE(bookmarks.slug, '') as "bookmark!",
                        languages.code as language_code,
                        languages.name as "language_name!: String",
                        word_classes.abbreviation as word_class_abbreviation,
                        created.username as created_by,
                        updated.username as updated_by
                    FROM words
                    LEFT JOIN bookmarks ON bookmarks.item = words.id AND bookmarks.resource = 'lemma'
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
                cognacy: Cognacy {
                    id: cognacy.id,
                    inner: filtered_inner,
                },
                words: words_map,
            }))
        } else {
            Ok(None)
        }
    }

    /// Compute topological levels for cognacy graph using Kahn's algorithm
    /// with maximum depth strategy (words placed at deepest reachable level)
    #[allow(dead_code)]
    pub async fn get_leveled_cognacy(&self, word: &Word) -> AppResult<Option<LeveledCognacy>> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let cognacy_full = self.get_cognacy(word).await?;

        let Some(cognacy_full) = cognacy_full else {
            return Ok(None);
        };

        let edges = match &cognacy_full.cognacy.inner {
            CognacyInner::V1(schema) => schema.edges.clone(),
        };

        if edges.is_empty() {
            // no edges, just return the single word at level 0
            return Ok(Some(LeveledCognacy {
                levels: vec![vec![word.id]],
                edges: vec![],
                words: cognacy_full.words,
            }));
        }

        // Build adjacency list and in-degree map
        let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
        let mut all_nodes: HashSet<Uuid> = HashSet::new();

        for edge in &edges {
            adjacency
                .entry(edge.antecedent)
                .or_default()
                .push(edge.consequent);
            all_nodes.insert(edge.antecedent);
            all_nodes.insert(edge.consequent);
            *in_degree.entry(edge.consequent).or_default() += 1;
            in_degree.entry(edge.antecedent).or_default();
        }

        // Track node levels (maximum depth strategy)
        let mut node_level: HashMap<Uuid, usize> = HashMap::new();
        let mut queue: VecDeque<Uuid> = VecDeque::new();

        // Start with nodes that have in-degree 0 (roots)
        for &node in &all_nodes {
            if in_degree.get(&node).copied().unwrap_or(0) == 0 {
                queue.push_back(node);
                node_level.insert(node, 0);
            }
        }

        // Process queue using Kahn's algorithm
        let mut processed_in_degree = in_degree.clone();
        while let Some(node) = queue.pop_front() {
            let current_level = node_level.get(&node).copied().unwrap_or(0);

            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    // Update neighbor's level to max(current, parent_level + 1)
                    let new_level = current_level + 1;
                    let existing_level = node_level.get(&neighbor).copied().unwrap_or(0);
                    node_level.insert(neighbor, existing_level.max(new_level));

                    // Decrease in-degree and add to queue when all parents processed
                    if let Some(degree) = processed_in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        // Group nodes by level
        let max_level = node_level.values().max().copied().unwrap_or(0);
        let mut levels: Vec<Vec<Uuid>> = vec![Vec::new(); max_level + 1];

        for (node, level) in node_level {
            levels[level].push(node);
        }

        Ok(Some(LeveledCognacy {
            levels,
            edges,
            words: cognacy_full.words,
        }))
    }
}

repo_from_parts!(WordRelationRepository);
