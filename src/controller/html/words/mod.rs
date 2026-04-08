mod create;
mod delete;
mod derive;
mod edit;
mod fragments;
mod search;
mod view;

pub use fragments::*;

use axum::Router;

use crate::util::AppState;

use axum::routing::{get, post};

pub const MAX_DEFINITIONS: usize = 10;

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
            "/languages/{language}/words/{slug}/{lemma}/derive-into-family",
            post(derive::derive_into_family_loan),
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
            "/languages/{language}/words/{slug}/{lemma}/derive-into-family",
            get(derive::derive_into_family_form),
        );

    (secure_routes, normal_routes)
}
