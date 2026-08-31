use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};

use crate::{
    attempt,
    controller::html::{okay, render_generic_error, render_template},
    get_user,
    model::{
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        words::{Word, WordRepository},
    },
    util::{AppState, BackQuery, extract_session::Session},
};

#[derive(Template)]
#[template(path = "words/delete.html")]
#[allow(dead_code)]
struct DeleteWordTemplate {
    current_user: Option<User>,
    language: Language,
    word: Word,
    can_edit_language: bool,
    can_delete_language: bool,
    will_create_audit_log: bool,
    back: String,
}

pub(super) async fn delete_word_form(
    s: Session,
    State(state): State<AppState>,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&user), language.id, &slug, lemma)
            .await
    );

    let can_edit_language = attempt!(
        s,
        permissions
            .can_edit_language(Some(&user), &language.id)
            .await
    );
    let can_delete_language = attempt!(
        s,
        permissions
            .can_delete_language(Some(&user), &language.id)
            .await
    );

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_language(&state, &user, language.id).await;

    let template = DeleteWordTemplate {
        current_user: Some(user),
        language,
        word,
        can_edit_language,
        can_delete_language,
        will_create_audit_log,
        back: back_query.back.unwrap_or_default(),
    };

    okay(render_template(template))
}

pub(super) async fn delete_word_submit(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
    Query(back_query): Query<BackQuery>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);

    match words
        .delete_by_lemma(&user, language.id, &slug, lemma)
        .await
    {
        Ok(_) => {
            let fallback = format!("/languages/{}/words", language_code);
            let redirect = crate::util::internal_back_or(back_query.back.as_deref(), &fallback);
            (
                StatusCode::SEE_OTHER,
                Redirect::to(&redirect).into_response(),
            )
        }
        Err(e) => render_generic_error(s, e).await,
    }
}
