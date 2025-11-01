use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{query, query_as, FromRow};
use uuid::Uuid;

use crate::{err::{bad_request, AppError, AppResult}, model::{language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, users::User, words::{Word, WordRepository}}, pagination::{PaginatedRequest, PaginatedResponse}, util::{ensure_verified, repo_from_parts, AppState}};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CognacyRelationKindV1 {
    Derived,
    Descendant,
    Compound,
    Calque,
    Borrowed
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CognacyEdgeV1 {
    pub antecedent: Uuid,
    pub consequent: Uuid,
    pub kind: CognacyRelationKindV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognacySchemaV1 {
    pub edges: Vec<CognacyEdgeV1>,
    pub schema_version: i32,
}

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
    pub antecedent: Option<Uuid>,
    pub consequent: Option<Uuid>,
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
pub enum Cognacy {
    V1(CognacySchemaV1),
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

        let Ok(cognacy_relation_kind) = CognacyRelationKindV1::try_from(relation.kind) else {
            // just add the relation, this doesn't affect cognacy graphs
            tx.commit().await?;
            return Ok(word_relation);
        };
        // aaaa i need Option<Future> -> Future<Option>
        let (antecedent_cognacy, consequent_cognacy) = tokio::try_join!(
            self.find_cognacy(&antecedent),
            self.find_cognacy(&consequent),
        )?;

        match (antecedent_cognacy, consequent_cognacy) {
            (Some(antecedent_cognacy), Some(consequent_cognacy)) => {
                let Cognacy::V1(antecedent_schema) = antecedent_cognacy else {
                    unimplemented!()
                };
                let Cognacy::V1(consequent_schema) = consequent_cognacy else {
                    unimplemented!()
                };
                // both words are already in cognacy graphs; need to merge the graphs
                unimplemented!()
            }
            (Some(antecedent_cognacy), None) => {
                let Cognacy::V1(antecedent_schema) = antecedent_cognacy else {
                    unimplemented!()
                };
                // antecedent is in a cognacy graph, add consequent to it
                unimplemented!()
            }
            (None, Some(consequent_cognacy)) => {
                let Cognacy::V1(consequent_schema) = consequent_cognacy else {
                    unimplemented!()
                };
                // consequent is in a cognacy graph, add antecedent to it
                unimplemented!()
            }
            (None, None) => {
                // neither word is in a cognacy graph, create a new one
                // luckily, no need for complex merging logic or checking that
                // we really have a DAG :)

                let edges = vec![
                    CognacyEdgeV1 {
                        antecedent: antecedent.id,
                        consequent: consequent.id,
                        kind: cognacy_relation_kind,
                    }
                ];
                
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

                tx.commit().await?;

                Ok(word_relation)
            }
        }
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
                        Ok(Some(Cognacy::V1(schema)))
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

    pub async fn search(&self, pagination: PaginatedRequest, search: SearchWordRelations, word: &Word) -> AppResult<PaginatedResponse<WordRelationSearchResult>> {
        println!("searching word relations for word id: {:?}", word.id);
        
        let items = query!(
            r#"
                SELECT DISTINCT ON (words.id)
                    words.*,
                    bookmarks.slug as "bookmark: String",
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
                definition: record.definition,
                ipa: record.ipa,
                notes: record.notes,
                extra: record.extra,
                created_at: record.created_at,
                updated_at: record.updated_at,
                created_by: record.created_by,
                updated_by: record.updated_by,
                bookmark: record.bookmark,
            };
            let direction = RelationDirection::from_str(&record.direction).unwrap();
            WordRelationSearchResult {
                kind: record.kind,
                related_word,
                direction,
            }
        }).collect();


        let total = count.unwrap_or(0) as i64;
        let has_more = (i64::from(pagination.offset) + items.len() as i64) < total;

        Ok(PaginatedResponse {
            items,
            total,
            offset: pagination.offset,
            limit: pagination.limit,
            has_more,
        })
    }
}

repo_from_parts!(WordRelationRepository);