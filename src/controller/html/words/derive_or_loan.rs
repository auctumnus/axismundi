use askama::Template;
use axum::{
    extract::Path,
    http::StatusCode,
    response::Response,
};
use futures::TryFutureExt;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    get_user,
    model::{
        language_permissions::LanguagePermissionRepository,
        languages::{Language, LanguageRepository},
        users::User,
        words::{WordRepository, WordWithMeta},
    },
    util::extract_session::Session,
};

#[derive(Template)]
#[template(path = "words/derive-or-loan.html")]
#[allow(dead_code)]
struct DeriveOrLoanTemplate {
    current_user: Option<User>,
    language: Language,
    word: WordWithMeta,
    can_edit_language: bool,
}

pub(super) async fn derive_or_loan_form(
    s: Session,
    languages: LanguageRepository,
    words: WordRepository,
    permissions: LanguagePermissionRepository,
    Path((language_code, slug, lemma)): Path<(String, String, i32)>,
) -> (StatusCode, Response) {
    let current_user = get_user!(&s);
    let language = attempt!(s, languages.find_by_code(&language_code).await);
    let word = attempt!(
        s,
        words
            .find_by_slug_and_lemma(Some(&current_user), language.id, &slug, lemma)
            .and_then(|word| words.materialize(word, Some(&current_user)))
            .await
    );

    let can_edit_language = attempt!(
        s,
        permissions
            .can_edit_language(Some(&current_user), &language.id)
            .await
    );

    let template = DeriveOrLoanTemplate {
        current_user: Some(current_user),
        language,
        word,
        can_edit_language,
    };

    okay(render_template(template))
}
