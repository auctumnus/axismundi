use crate::{
    err::AppResult,
    model::bookmarks::{BookmarkRepository, LinkType},
    util::AppState,
};
use axum::{
    extract::Path,
    response::Redirect,
};

pub fn create_router() -> (axum::Router<crate::util::AppState>, axum::Router<crate::util::AppState>) {
    let normal_routes = axum::Router::new().route(
        "/bookmarks/{slug}",
        axum::routing::get(get_bookmark),
    );

    (axum::Router::new(), normal_routes)
}

async fn get_bookmark(
    Path(slug): Path<String>,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> AppResult<Redirect> {
    let bookmarks = BookmarkRepository::new(state);
    let bookmark = bookmarks.get_by_slug(&slug).await?;
    let to = bookmarks.resolve_bookmark(bookmark.item, bookmark.resource, LinkType::Web).await?;
    Ok(Redirect::temporary(&to))
}
