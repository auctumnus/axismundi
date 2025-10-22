use std::sync::Arc;

use crate::{
    err::{bad_request, forbidden, not_found, unauthorized_no_session, AppError, AppResult}, model::{
        language::{CreateLanguage, Language, LanguageRepository, LanguageSearch, UpdateLanguage},
        language_invite::{CreateLanguageInvite, LanguageInvite, LanguageInviteRepository, PermissionLevel},
        language_permission::{CreateLanguagePermission, LanguagePermission, LanguagePermissionRepository},
        session::{SessionObj, SessionRepository},
        user::{CreateUser, User, UserRepository, UserSearch},
        word::{CreateWord, UpdateWord, Word, WordRepository, WordSearch},
        word_class::{CreateWordClass, UpdateWordClass, WordClass, WordClassRepository, WordClassSearch},
    }, pagination::{PaginatedRequest, PaginatedResponse}, util::{
        extract_session::{Session, SESSION_COOKIE_NAME}, s3::S3, AppState
    }
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use axum_extra::extract::{CookieJar, Multipart, cookie::Cookie};
use chrono::{DateTime, Utc};
use governor::middleware::NoOpMiddleware;
use serde::{Deserialize, Serialize};
use tower_governor::governor::GovernorConfig;
use uuid::Uuid;

type ApiResponse<T> = AppResult<T>;
type PaginatedApiResponse<T> = AppResult<PaginatedResponse<T>>;

pub fn create_api_controller() -> Router<AppState> {
    let secure_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::secure());
    let normal_governor = Arc::new(GovernorConfig::<_, NoOpMiddleware>::default());

    let secure_limiter = secure_governor.limiter().clone();
    let normal_limiter = normal_governor.limiter().clone();
    let interval = std::time::Duration::from_secs(60);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            secure_limiter.retain_recent();
            normal_limiter.retain_recent();
        }
    });

    let secure_routes = Router::<AppState>::new()
        // users
        .route("/users", post(create_user))
        .route("/users/{id}/verify", post(verify_user))
        .route("/users/{username}/profile-picture", put(upload_profile_picture))
        // sessions
        .route("/sessions", post(login))
        .route("/sessions", get(get_sessions))
        // languages
        .route("/languages", post(create_language))
        .route("/languages/{code}", put(edit_language))
        .route("/languages/{code}", delete(delete_language))
        // language permissions
        .route("/languages/{code}/permissions/{username}", put(edit_user_permissions))
        .route("/languages/{code}/permissions/{username}", delete(delete_user_permissions))
        // language invites
        .route("/languages/{code}/invites/{username}", post(invite_user_to_language))
        .route("/languages/{code}/invites/{username}", delete(delete_language_invite))
        .route("/languages/{code}/accept-invite", post(accept_language_invite))
        // word classes
        .route("/languages/{code}/word-classes", post(create_word_class))
        .route("/languages/{code}/word-classes/{abbreviation}", put(edit_word_class))
        .route("/languages/{code}/word-classes/{abbreviation}", delete(delete_word_class))
        // words
        .route("/languages/{code}/words", post(create_word))
        .route("/languages/{code}/words/{slug}", put(edit_word))
        .route("/languages/{code}/words/{slug}", delete(delete_word))
        .layer(tower_governor::GovernorLayer {
            config: secure_governor,
        });

    let normal_routes = Router::<AppState>::new()
        // users
        .route("/users/{username}", get(get_user))
        .route("/users", get(search_users))
        // languages
        .route("/languages/{code}", get(get_language))
        .route("/languages", get(list_languages))
        .route("/languages/{code}/owner", get(get_language_owner))
        .route("/languages/{code}/editors", get(get_language_editors))
        .route("/languages/{code}/permissions", get(get_language_permissions))
        .route("/languages/{code}/permissions/{username}", get(get_user_language_permissions))
        // word classes
        .route("/languages/{code}/word-classes", get(list_word_classes))
        .route("/languages/{code}/word-classes/{abbreviation}", get(get_word_class))
        // words
        .route("/languages/{code}/words", get(list_words))
        .layer(tower_governor::GovernorLayer {
            config: normal_governor,
        });

    Router::<AppState>::new()
        .merge(secure_routes)
        .merge(normal_routes)
}

async fn get_user(
    State(AppState { pool, .. }): State<AppState>,
    Path(username): Path<String>,
) -> ApiResponse<Json<User>> {
    UserRepository::new(pool)
        .find_by_username(&username)
        .await
        .map(Json)
}

async fn create_user(
    State(AppState { pool, .. }): State<AppState>,
    Json(user): Json<CreateUser>,
) -> ApiResponse<Json<User>> {
    let user_repo = UserRepository::new(pool.clone());
    user_repo.create(user).await.map(Json)
}

#[derive(Deserialize)]
struct LoginCredentials {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    expires_at: DateTime<Utc>,
}

async fn login(
    jar: CookieJar,
    sessions: SessionRepository,
    Json(credentials): Json<LoginCredentials>,
) -> ApiResponse<(CookieJar, Json<LoginResponse>)> {
    sessions
        .login(&credentials.email, &credentials.password)
        .await
        .map(|(token, session)| {
            let jar = jar.add(
                Cookie::build((SESSION_COOKIE_NAME, token.clone()))
                    .path("/")
                    .http_only(true)
                    .secure(true),
            );
            let response = Json(LoginResponse {
                token,
                expires_at: session.expires_at,
            });

            (jar, response)
        })
}

#[derive(Deserialize)]
struct VerifyEmail {
    token: String,
    email: String,
}

async fn verify_user(
    users: UserRepository,
    Path(id): Path<Uuid>,
    Json(VerifyEmail { token, email }): Json<VerifyEmail>,
) -> ApiResponse<StatusCode> {
    users
        .verify(id, &email, &token)
        .await
        .map(|_| StatusCode::OK)
}

async fn get_sessions(
    s: Session,
    sessions: SessionRepository,
) -> ApiResponse<Json<Vec<SessionObj>>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    sessions.find_by_user_id(session.user_id).await.map(Json)
}

const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5MB

#[derive(Serialize)]
struct ProfilePictureUploadResponse {
    profile_picture_url: String,
}
async fn upload_profile_picture(
    s: Session,
    users: UserRepository,
    Path(username): Path<String>,
    mut multipart: Multipart,
) -> ApiResponse<Json<ProfilePictureUploadResponse>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let user = users.find_by_username(&username).await?;

    // Check if user is uploading their own profile picture
    if session.user_id != user.id {
        return Err(AppError::new(
            "You can only upload your own profile picture".to_string(),
            StatusCode::FORBIDDEN,
        ));
    }

    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Multipart error: {e}")))?
    {
        let field_name = field.name().unwrap_or("");

        if field_name == "image" {
            let data = field
                .bytes()
                .await
                .map_err(|e| bad_request(format!("Field bytes error: {e}")))?;
            file_data = Some(data.to_vec());
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| bad_request("No image file provided"))?;

    // Upload to S3
    let object_key = S3.upload_profile_picture(user.id, &file_data).await?;

    // Update user record with new profile picture object key
    match users.update_profile_picture(user.id, &object_key).await? {
        Some(_) => {
            let profile_picture_url = S3.get_object_url(&object_key);
            Ok(Json(ProfilePictureUploadResponse {
                profile_picture_url,
            }))
        }
        None => Err(not_found("User not found")),
    }
}

#[derive(Deserialize)]
struct UserSearchQuery {
    q: Option<String>,
    created_before: Option<DateTime<Utc>>,
    created_after: Option<DateTime<Utc>>,
}

async fn search_users(
    users: UserRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<UserSearchQuery>,
) -> PaginatedApiResponse<User> {
    let search = UserSearch {
        pagination,
        text_query: query.q,
        created_before: query.created_before,
        created_after: query.created_after,
        verified: None,
    };

    users.search(search).await
}

// ===== languages =====

async fn create_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Json(create): Json<CreateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    if create.code == "search" {
        return Err(bad_request("cannot use 'search' as language code"));
    }

    let language = languages.create(create, session.user_id).await?;

    // grant owner permissions
    permissions.create(
        CreateLanguagePermission {
            language: language.id,
            user: session.user_id,
            permission: PermissionLevel::Owner,
            via: None,
        },
        session.user_id,
    ).await?;

    Ok(Json(language))
}

async fn get_language(
    languages: LanguageRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Language>> {
    languages.find_by_code(&code).await.map(Json)
}

#[derive(Deserialize)]
struct LanguageSearchQuery {
    owned_by: Option<String>,
    edited_by: Option<String>,
    q: Option<String>,
    created_before: Option<DateTime<Utc>>,
    created_after: Option<DateTime<Utc>>,
}

async fn list_languages(
    languages: LanguageRepository,
    users: UserRepository,
    permissions: LanguagePermissionRepository,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<LanguageSearchQuery>,
) -> PaginatedApiResponse<Language> {
    let edited_by = query.edited_by.map(|s| s.split(',').map(String::from).collect());

    let search = LanguageSearch {
        pagination,
        text_query: query.q,
        owned_by: query.owned_by,
        edited_by,
        created_before: query.created_before,
        created_after: query.created_after,
    };

    languages.search(search).await
}

async fn edit_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    Json(updates): Json<UpdateLanguage>,
) -> ApiResponse<Json<Language>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit this language"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit language"));
    }

    languages.update(language.id, updates, session.user_id).await.map(Json)
}

async fn delete_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete this language"));
    };

    if perm.permission != PermissionLevel::Owner {
        return Err(forbidden("only the owner can delete a language"));
    }

    languages.delete(language.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_language_owner(
    languages: LanguageRepository,
    users: UserRepository,
    Path(code): Path<String>,
) -> ApiResponse<axum::response::Redirect> {
    let language = languages.find_by_code(&code).await?;
    let owner = users.find_by_id(language.created_by).await?;
    Ok(axum::response::Redirect::to(&format!("/users/{}", owner.username)))
}

async fn get_language_editors(
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
) -> PaginatedApiResponse<LanguagePermission> {
    let language = languages.find_by_code(&code).await?;
    // TODO: implement pagination
    Ok(PaginatedResponse {
        items: vec![],
        pages_left: 0,
        next_cursor: None,
        previous_cursor: None,
    })
}

// ===== language permissions =====

async fn get_language_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    Path(code): Path<String>,
) -> ApiResponse<Json<Vec<LanguagePermission>>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to view permissions"));
    };

    if perm.permission != PermissionLevel::Owner && perm.permission != PermissionLevel::Admin {
        return Err(forbidden("only owners and admins can view all permissions"));
    }

    permissions.list_by_language(language.id).await.map(Json)
}

async fn get_user_language_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<Json<LanguagePermission>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(_) = user_perm else {
        return Err(forbidden("you don't have permission to view permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    target_perm.ok_or_else(|| not_found(format!("permission for user '{username}' on language '{code}'")))
        .map(Json)
}

#[derive(Deserialize)]
struct EditPermissionRequest {
    permission_level: PermissionLevel,
}

async fn edit_user_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<EditPermissionRequest>,
) -> ApiResponse<Json<LanguagePermission>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let requester_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(requester) = requester_perm else {
        return Err(forbidden("you don't have permission to edit permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    let Some(target) = target_perm else {
        return Err(bad_request("user doesn't have permissions for this language"));
    };

    // check permission table from api.md
    let can_edit = match (requester.permission, target.permission) {
        (PermissionLevel::Owner, PermissionLevel::Owner) => false,
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_edit {
        return Err(forbidden("you don't have permission to edit this user's permissions"));
    }

    permissions.update_permission(target.id, req.permission_level).await.map(Json)
}

async fn delete_user_permissions(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let requester_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(requester) = requester_perm else {
        return Err(forbidden("you don't have permission to delete permissions"));
    };

    let target_user = users.find_by_username(&username).await?;
    let target_perm = permissions.find_by_user_and_language(target_user.id, language.id).await?;

    let Some(target) = target_perm else {
        return Err(not_found("user doesn't have permissions for this language"));
    };

    // check if removing own permissions (always allowed except owner)
    if session.user_id == target_user.id {
        if requester.permission == PermissionLevel::Owner {
            return Err(forbidden("owner cannot remove their own permissions"));
        }
        permissions.delete(target.id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // check permission table from api.md
    let can_delete = match (requester.permission, target.permission) {
        (PermissionLevel::Owner, PermissionLevel::Owner) => false,
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_delete {
        return Err(forbidden("you don't have permission to delete this user's permissions"));
    }

    permissions.delete(target.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ===== language invites =====

#[derive(Deserialize)]
struct InviteRequest {
    permission_level: PermissionLevel,
}

async fn invite_user_to_language(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
    Json(req): Json<InviteRequest>,
) -> ApiResponse<Json<LanguageInvite>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let sender_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(sender) = sender_perm else {
        return Err(forbidden("you don't have permission to invite users"));
    };

    let recipient = users.find_by_username(&username).await?;

    // check if user already has permissions
    let existing = permissions.find_by_user_and_language(recipient.id, language.id).await?;
    if existing.is_some() {
        return Err(bad_request("user already has permissions for this language"));
    }

    // check if invite already exists
    let existing_invites = invites.list_by_language(language.id).await?;
    if existing_invites.iter().any(|i| i.recipient == recipient.id && i.accepted_at.is_none()) {
        return Err(bad_request("invite already exists for this user"));
    }

    // check permission to invite
    let can_invite = match (sender.permission, req.permission_level) {
        (PermissionLevel::Owner, _) => true,
        (PermissionLevel::Admin, PermissionLevel::Editor) => true,
        _ => false,
    };

    if !can_invite {
        return Err(forbidden("you don't have permission to send this invite"));
    }

    invites.create(
        CreateLanguageInvite {
            language: language.id,
            recipient: recipient.id,
            permissions: req.permission_level,
        },
        session.user_id,
    ).await.map(Json)
}

async fn delete_language_invite(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    users: UserRepository,
    Path((code, username)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let recipient = users.find_by_username(&username).await?;

    let existing_invites = invites.list_by_language(language.id).await?;
    let invite = existing_invites.iter().find(|i| i.recipient == recipient.id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("invite not found"));
    };

    // if the recipient is deleting (rejecting) their own invite
    if session.user_id == recipient.id {
        invites.delete(invite.id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    // otherwise check sender permissions
    let sender_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(sender) = sender_perm else {
        return Err(forbidden("you don't have permission to delete invites"));
    };

    let can_delete = match sender.permission {
        PermissionLevel::Owner => true,
        PermissionLevel::Admin => invite.sender != language.created_by,
        _ => false,
    };

    if !can_delete {
        return Err(forbidden("you don't have permission to delete this invite"));
    }

    invites.delete(invite.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn accept_language_invite(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    invites: LanguageInviteRepository,
    Path(code): Path<String>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;

    let existing_invites = invites.list_by_language(language.id).await?;
    let invite = existing_invites.iter().find(|i| i.recipient == session.user_id && i.accepted_at.is_none());

    let Some(invite) = invite else {
        return Err(not_found("no pending invite found"));
    };

    // create permission
    permissions.create(
        CreateLanguagePermission {
            language: language.id,
            user: session.user_id,
            permission: invite.permissions,
            via: Some(invite.id),
        },
        invite.sender,
    ).await?;

    // mark invite as accepted
    invites.accept(invite.id).await?;

    Ok(StatusCode::OK)
}

// ===== word classes =====

async fn create_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    Json(mut create): Json<CreateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to create word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot create word classes"));
    }

    if create.abbreviation == "search" {
        return Err(bad_request("cannot use 'search' as abbreviation"));
    }

    create.language = language.id;
    word_classes.create(create, session.user_id).await.map(Json)
}

async fn list_word_classes(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<SearchQuery>,
) -> PaginatedApiResponse<WordClass> {
    let language = languages.find_by_code(&code).await?;

    let search = WordClassSearch {
        pagination,
        text_query: query.q,
    };

    word_classes.search(language.id, search).await
}

async fn get_word_class(
    languages: LanguageRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<Json<WordClass>> {
    let language = languages.find_by_code(&code).await?;
    let classes = word_classes.list_by_language(language.id).await?;

    classes.iter()
        .find(|c| c.abbreviation == abbreviation)
        .cloned()
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))
        .map(Json)
}

async fn edit_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
    Json(updates): Json<UpdateWordClass>,
) -> ApiResponse<Json<WordClass>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit word classes"));
    }

    let classes = word_classes.list_by_language(language.id).await?;
    let class = classes.iter().find(|c| c.abbreviation == abbreviation)
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))?;

    word_classes.update(class.id, updates, session.user_id).await.map(Json)
}

async fn delete_word_class(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    word_classes: WordClassRepository,
    Path((code, abbreviation)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete word classes"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot delete word classes"));
    }

    let classes = word_classes.list_by_language(language.id).await?;
    let class = classes.iter().find(|c| c.abbreviation == abbreviation)
        .ok_or_else(|| not_found(format!("word class '{abbreviation}'")))?;

    word_classes.delete(class.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ===== words =====

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Deserialize)]
struct WordSearchQuery {
    q: Option<String>,
    word_class: Option<String>,
}

async fn create_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path(code): Path<String>,
    Json(mut create): Json<CreateWord>,
) -> ApiResponse<Json<Word>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to create words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot create words"));
    }

    create.language = language.id;
    words.create(create, session.user_id).await.map(Json)
}

async fn list_words(
    languages: LanguageRepository,
    words: WordRepository,
    word_classes: WordClassRepository,
    Path(code): Path<String>,
    pagination: PaginatedRequest,
    axum::extract::Query(query): axum::extract::Query<WordSearchQuery>,
) -> PaginatedApiResponse<Word> {
    let language = languages.find_by_code(&code).await?;

    let word_class_uuid = if let Some(ref abbr) = query.word_class {
        let classes = word_classes.list_by_language(language.id).await?;
        classes.iter()
            .find(|c| c.abbreviation == *abbr)
            .map(|c| c.id)
    } else {
        None
    };

    let search = WordSearch {
        pagination,
        text_query: query.q,
        word_class: word_class_uuid,
    };

    words.search(language.id, search).await
}

async fn edit_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
    Json(updates): Json<UpdateWord>,
) -> ApiResponse<Json<Word>> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to edit words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot edit words"));
    }

    let word = words.find_by_slug(language.id, &slug).await?;
    words.update(word.id, updates, session.user_id).await.map(Json)
}

async fn delete_word(
    s: Session,
    languages: LanguageRepository,
    permissions: LanguagePermissionRepository,
    words: WordRepository,
    Path((code, slug)): Path<(String, String)>,
) -> ApiResponse<StatusCode> {
    let Some(session) = s.session() else {
        return Err(unauthorized_no_session());
    };

    let language = languages.find_by_code(&code).await?;
    let user_perm = permissions.find_by_user_and_language(session.user_id, language.id).await?;

    let Some(perm) = user_perm else {
        return Err(forbidden("you don't have permission to delete words"));
    };

    if perm.permission == PermissionLevel::Viewer {
        return Err(forbidden("viewers cannot delete words"));
    }

    let word = words.find_by_slug(language.id, &slug).await?;
    words.delete(word.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
