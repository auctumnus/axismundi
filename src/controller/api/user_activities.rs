use crate::{
    err::{AppResult, unauthorized_no_session},
    model::{
        languages::LanguageRepository,
        user_activities::{UserActivity, UserActivityRepository},
        users::UserRepository,
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::extract_session::Session,
};
use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get},
};
use uuid::Uuid;

pub fn create_router() -> axum::Router<crate::util::AppState> {
    axum::Router::new()
        .route("/users/{username}/activities", get(list_activities_by_user))
        .route(
            "/languages/{code}/activities",
            get(list_activities_by_language),
        )
        .route("/activities/{id}", delete(delete_activity))
}

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub async fn list_activities_by_user(
    s: Session,
    users: UserRepository,
    activities: UserActivityRepository,
    Path(username): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<UserActivity> {
    let user = users.find_by_username(&username).await?;

    // Note: language_id is None here since we're not filtering by language
    activities
        .list_by_user(s.user(), user.id, None, pagination)
        .await
}

pub async fn list_activities_by_language(
    s: Session,
    languages: LanguageRepository,
    activities: UserActivityRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<UserActivity> {
    let language = languages.find_by_code(&code).await?;

    activities
        .list_by_language(s.user(), language.id, pagination)
        .await
}

pub async fn delete_activity(
    s: Session,
    activities: UserActivityRepository,
    Path(id): Path<Uuid>,
) -> ApiResponse<StatusCode> {
    let Some(requestor) = s.user() else {
        return Err(unauthorized_no_session());
    };

    activities.delete(requestor, id).await?;

    Ok(StatusCode::NO_CONTENT)
}
