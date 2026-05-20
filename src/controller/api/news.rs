use crate::{
    err::{AppResult, unauthorized_no_session},
    model::news::{CreateNews, News, NewsRepository, NewsSearch, UpdateNews},
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use validator::Validate;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/news", post(create_news))
        .route("/news", get(search_news))
        .route("/news/{slug}", get(get_news))
        .route("/news/{slug}", put(edit_news))
        .route("/news/{slug}", delete(delete_news))
        .route("/news/{slug}/publish", post(publish_news))
        .route("/news/{slug}/unpublish", post(unpublish_news))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn create_news(
    s: Session,
    news: NewsRepository,
    Json(req): Json<CreateNews>,
) -> ApiResponse<Json<News>> {
    req.validate()?;
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    news.create(requestor, req).await.map(Json)
}

pub async fn get_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> ApiResponse<Json<News>> {
    let article = news.find_by_slug_for(&slug, s.user()).await?;
    Ok(Json(article))
}

pub async fn search_news(
    s: Session,
    news: NewsRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<NewsSearch>,
) -> PaginatedApiResponse<News> {
    news.search(pagination, query, s.user()).await
}

pub async fn edit_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
    Json(updates): Json<UpdateNews>,
) -> ApiResponse<Json<News>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    let id = news.find_by_slug_for(&slug, Some(requestor)).await?.id;
    news.update(requestor, id, updates).await.map(Json)
}

pub async fn delete_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    let article = news.find_by_slug_for(&slug, Some(requestor)).await?;
    news.delete(requestor, article).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn publish_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> ApiResponse<Json<News>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    let article = news.find_by_slug_for(&slug, Some(requestor)).await?;
    news.publish(requestor, article.id).await.map(Json)
}

pub async fn unpublish_news(
    s: Session,
    news: NewsRepository,
    Path(slug): Path<String>,
) -> ApiResponse<Json<News>> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };
    let article = news.find_by_slug_for(&slug, Some(requestor)).await?;
    news.unpublish(requestor, article.id).await.map(Json)
}
