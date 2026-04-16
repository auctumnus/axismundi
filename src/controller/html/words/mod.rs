mod create;
mod delete;
mod derive;
mod derive_or_loan;
mod edit;
mod fragments;
mod loan;
mod search;
mod view;

pub use fragments::*;

use axum::Router;
use uuid::Uuid;

use crate::{
    err::{AppError, AppResult, bad_request},
    model::sound_change_sets::SoundChangeSetRepository,
    util::AppState,
};

use axum::routing::{get, post};

pub const MAX_DEFINITIONS: usize = 10;

async fn estimate_ipa(
    sets: SoundChangeSetRepository,
    ipa_estimator: &Uuid,
    word: &str,
) -> AppResult<String> {
    sets.run_from_db(ipa_estimator, vec![word.to_string()])
        .await
        .and_then(|results| {
            if let Some(errors) = results.errors {
                if errors.is_empty() {
                    Ok(results.output_words.first().cloned().unwrap_or_default())
                } else {
                    Err(bad_request(format!(
                        "IPA estimation failed: {}",
                        errors
                            .into_iter()
                            .map(|e| e.message)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
            } else {
                Ok(results.output_words.first().cloned().unwrap_or_default())
            }
        })
        .map_err(|e| {
            let mut validation_errors = validator::ValidationErrors::new();
            validation_errors.add(
                "ipa",
                validator::ValidationError {
                    code: "custom".into(),
                    message: Some(e.message.into()),
                    params: std::collections::HashMap::new(),
                },
            );

            AppError {
                message: "Failed to estimate IPA".into(),
                status_code: e.status_code,
                validation_errors: Some(validation_errors),
                extra: None,
            }
        })
}

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/languages/{language}/new-word", get(create::new_word))
        .route(
            "/languages/{language}/new-word",
            post(create::new_word_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/edit",
            get(edit::edit_word),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/edit",
            post(edit::edit_word_submit),
        )
        .route(
            "/languages/{language}/new-word/estimate-ipa",
            post(create::estimate_ipa_new_word),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/estimate-ipa",
            post(edit::estimate_ipa_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/delete",
            post(delete::delete_word_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/derive",
            post(derive::derive_submit),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/loan",
            post(loan::loan_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/languages/{language}/words", get(search::word_search))
        .route(
            "/languages/{language}/words/{slug}",
            get(view::view_lemmata),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}",
            get(view::view_lemma),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/delete",
            get(delete::delete_word_form),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/derive",
            get(derive::derive_form),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/loan",
            get(loan::loan_form),
        )
        .route(
            "/languages/{language}/words/{slug}/{lemma}/derive-or-loan",
            get(derive_or_loan::derive_or_loan_form),
        );

    (secure_routes, normal_routes)
}
