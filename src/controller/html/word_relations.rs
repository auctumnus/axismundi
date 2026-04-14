use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{self, okay, render_generic_error, render_template},
    err::{AppError, bad_request},
    get_user,
    model::{
        bookmarks::BookmarkRepository,
        language_invites::PermissionLevel,
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        word_relations::{
            CreateWordRelation, SearchWordRelations, WordRelationRepository,
            WordRelationSearchResult, WordRelationType,
        },
        words::{Word, WordRepository},
    },
    pagination::PaginatedRequest,
    util::{
        AppState,
        extract_session::Session,
        search_template::{SearchTemplateArgs, make_search_layout},
    },
};

#[derive(Template)]
#[template(path = "words/relations/add_relation.html")]
#[allow(dead_code)]
struct AddRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    error: Option<AppError>,
    will_create_audit_log: bool,
}

async fn add_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        return render_generic_error(s, bad_request("You don't have permission to add relations"))
            .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    let template = AddRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        error: None,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

#[allow(clippy::too_many_arguments)]
async fn add_relation_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    bookmarks: BookmarkRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Form(form): Form<AddRelationForm>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Check permission on source language
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        let template = AddRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            error: Some(bad_request("You don't have permission to add relations")),
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Look up the target word by bookmark
    let bookmark_result = bookmarks.get_by_slug(&form.target_bookmark).await;

    let target_word = match bookmark_result {
        Ok(bookmark) => {
            // Get the target word using the bookmark's item UUID
            match words.find_by_id(bookmark.item).await {
                Ok(w) => w,
                Err(e) => {
                    let template = AddRelationTemplate {
                        current_user: Some(current_user.clone()),
                        language,
                        word,
                        error: Some(e),
                        will_create_audit_log,
                    };
                    let body = render_template(template);
                    return okay(body);
                }
            }
        }
        Err(e) => {
            let template = AddRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                error: Some(e),
                will_create_audit_log,
            };
            let body = render_template(template);
            return okay(body);
        }
    };

    // Check permission on target language
    let has_target_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(
                    current_user.id,
                    target_word.language,
                    PermissionLevel::Editor
                )
                .await
        );
    if !has_target_permission {
        let template = AddRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            error: Some(bad_request(
                "You don't have permission to edit the target word's language",
            )),
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Create the relation
    let relation = CreateWordRelation {
        antecedent: word.id,
        consequent: target_word.id,
        kind: form.kind,
    };

    let relation_result = word_relations.create(&current_user, relation).await;

    match relation_result {
        Ok(_) => {
            // Redirect to etymology page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => {
            let template = AddRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                error: Some(e),
                will_create_audit_log,
            };
            let body = render_template(template);
            okay(body)
        }
    }
}

#[derive(Debug, Deserialize)]
struct AddRelationForm {
    kind: WordRelationType,
    target_bookmark: String,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/list_header.html")]
struct RelationsHeader<'a> {
    user_has_permission: bool,
    language: &'a Language,
    word: &'a Word,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/query.html")]
struct RelationsQueryTemplate {
    query: SearchWordRelations,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/card.html")]
struct RelationCard<'a> {
    relation: WordRelationSearchResult,
    back_url: &'a str,
    current_word_language_code: &'a str,
    current_word_slug: &'a str,
    current_word_lemma: i32,
    user_has_permission: bool,
}

#[derive(Template)]
#[template(path = "words/relations/fragments/breadcrumb.html")]
struct RelationsBreadcrumb<'a> {
    language: &'a Language,
    word: &'a Word,
}

#[allow(clippy::too_many_arguments)]
async fn view_word_relations(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(query): Query<SearchWordRelations>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let current_user = s.user().cloned();
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(current_user.as_ref(), language.id, &slug, lemma)
            .await
    );

    let user_has_permission = if let Some(user) = &current_user {
        let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
            .await
            .unwrap_or(false);

        is_admin_or_mod
            || language_permissions
                .has_permission(user.id, language.id, PermissionLevel::Editor)
                .await
                .unwrap_or(false)
    } else {
        false
    };

    let search_action = format!(
        "/languages/{}/words/{}/{}/relations",
        language.code, word.slug, word.lemma
    );

    let back_url = crate::util::back_url(&search_action, &pagination, &query);

    let header = RelationsHeader {
        user_has_permission,
        language: &language,
        word: &word,
    };

    let query_template = RelationsQueryTemplate {
        query: query.clone(),
    };

    let breadcrumbs = RelationsBreadcrumb {
        language: &language,
        word: &word,
    };

    let footer = html::languages::Footer {
        language: &language,
        can_edit_language: user_has_permission,
    };

    let results = word_relations
        .search(pagination.clone(), query.clone(), &word)
        .await;

    let render_item = |relation: &WordRelationSearchResult| RelationCard {
        relation: relation.clone(),
        back_url: &back_url,
        current_word_language_code: &language.code,
        current_word_slug: &word.slug,
        current_word_lemma: word.lemma,
        user_has_permission,
    };

    let template = make_search_layout(SearchTemplateArgs {
        current_user,
        header,
        query_template,
        query,
        results,
        pagination,
        search_name: "relations",
        search_action,
        render_item,
    })
    .with_breadcrumbs(breadcrumbs)
    .with_footer(footer);

    let status = template.status();
    (status, render_template(template))
}

#[derive(Template)]
#[template(path = "words/relations/delete_relation.html")]
#[allow(dead_code)]
struct DeleteRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    related_word: Word,
    related_word_language_code: String,
    user_has_permission: bool,
    will_create_audit_log: bool,
}

async fn delete_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to delete relations"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    let template = DeleteRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        related_word,
        related_word_language_code: related_language_code.clone(),
        user_has_permission: has_permission,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

async fn delete_relation_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    // Check permission
    let has_permission = attempt!(
        s,
        language_permissions
            .has_permission(current_user.id, language.id, PermissionLevel::Editor)
            .await
    );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to delete relations"),
        )
        .await;
    }

    // Delete the relation
    match word_relations
        .delete(&current_user, &word, &related_word)
        .await
    {
        Ok(()) => {
            // Redirect back to the word page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => render_generic_error(s, e).await,
    }
}

#[derive(Template)]
#[template(path = "words/relations/edit_relation.html")]
#[allow(dead_code)]
struct EditRelationTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    related_word: Word,
    related_language: Language,
    related_word_language_code: String,
    relation_kind: String,
    error: Option<AppError>,
    user_has_permission: bool,
    user_has_permission_on_related: bool,
    will_create_audit_log: bool,
}

async fn edit_relation_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    // Check permission
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    if !has_permission {
        return render_generic_error(
            s,
            bad_request("You don't have permission to edit relations"),
        )
        .await;
    }

    let has_permission_on_related = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(
                    current_user.id,
                    related_language.id,
                    PermissionLevel::Editor
                )
                .await
        );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Find the existing relation to get its current kind
    let relation_kind = match word_relations.find_relation(&word, &related_word).await {
        Ok(relation) => relation.kind.to_string(),
        Err(e) => return render_generic_error(s, e).await,
    };

    let template = EditRelationTemplate {
        current_user: Some(current_user.clone()),
        language,
        word,
        related_word,
        related_language,
        related_word_language_code: related_language_code.clone(),
        relation_kind,
        error: None,
        user_has_permission: has_permission,
        user_has_permission_on_related: has_permission_on_related,
        will_create_audit_log,
    };

    let body = render_template(template);
    okay(body)
}

#[derive(Debug, Deserialize)]
struct EditRelationForm {
    kind: WordRelationType,
}

#[allow(clippy::too_many_arguments)]
async fn edit_relation_submit(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    word_relations: WordRelationRepository,
    language_permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma, related_language_code, related_slug, related_lemma)): Path<(
        String,
        String,
        i32,
        String,
        String,
        i32,
    )>,
    Form(form): Form<EditRelationForm>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .await
    );

    let related_language = attempt!(s, languages.find_by_code(&related_language_code).await);
    let related_word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(
                Some(&current_user),
                related_language.id,
                &related_slug,
                related_lemma
            )
            .await
    );

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, current_user.id)
        .await
        .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &current_user, language.id).await;

    // Check permission
    let has_permission = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(current_user.id, language.id, PermissionLevel::Editor)
                .await
        );
    let has_permission_on_related = is_admin_or_mod
        || attempt!(
            s,
            language_permissions
                .has_permission(
                    current_user.id,
                    related_language.id,
                    PermissionLevel::Editor
                )
                .await
        );
    if !has_permission {
        let template = EditRelationTemplate {
            current_user: Some(current_user.clone()),
            language,
            word,
            related_word,
            related_language,
            related_word_language_code: related_language_code.clone(),
            relation_kind: form.kind.to_string(),
            error: Some(bad_request("You don't have permission to edit relations")),
            user_has_permission: has_permission,
            user_has_permission_on_related: has_permission_on_related,
            will_create_audit_log,
        };
        let body = render_template(template);
        return okay(body);
    }

    // Update the relation
    match word_relations
        .update(&current_user, &word, &related_word, form.kind)
        .await
    {
        Ok(_) => {
            // Redirect back to the word page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!(
                    "/languages/{}/words/{}/{}",
                    language.code, word.slug, word.lemma
                ))
                .into_response(),
            )
        }
        Err(e) => {
            let template = EditRelationTemplate {
                current_user: Some(current_user.clone()),
                language,
                word,
                related_word,
                related_language,
                related_word_language_code: related_language_code.clone(),
                relation_kind: form.kind.to_string(),
                error: Some(e),
                user_has_permission: has_permission,
                user_has_permission_on_related: has_permission_on_related,
                will_create_audit_log,
            };
            let body = render_template(template);
            okay(body)
        }
    }
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route(
            "/languages/{language}/words/{slug}/{lemma}/add-relation",
            axum::routing::post(add_relation_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete",
            axum::routing::post(delete_relation_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit",
            axum::routing::post(edit_relation_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route(
            "/languages/{language}/words/{slug}/{lemma}/relations",
            axum::routing::get(view_word_relations),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/add-relation",
            axum::routing::get(add_relation_form),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/delete",
            axum::routing::get(delete_relation_form),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/relations/{related_language}/{related_slug}/{related_lemma}/edit",
            axum::routing::get(edit_relation_form),
        );

    (secure_routes, normal_routes)
}
