use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::{TypedHeader, headers::UserAgent};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::{
    attempt,
    controller::html::{okay, render_template},
    embed::{self, GenericEmbed, render_embed, truncate_description},
    err::AppError,
    get_user,
    model::{
        language_families::{
            CreateLanguageFamily, FamilyWithContributors, LanguageFamily, LanguageFamilyRepository,
            SearchLanguageFamilies,
        },
        language_family_invites::{LanguageFamilyInvite, LanguageFamilyInviteRepository},
        language_family_members::LanguageFamilyMemberRepository,
        language_family_permissions::LanguageFamilyPermissionRepository,
        language_invites::PermissionLevel,
        users::{User, UserRepository},
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::<AppState>::new()
        .route("/new-language-family", post(new_language_family_submit))
        .route(
            "/language-families/{code}/edit",
            post(edit_language_family_submit),
        )
        .route(
            "/language-families/{code}/delete",
            post(delete_language_family_submit),
        );

    let normal_routes = Router::<AppState>::new()
        .route("/language-families", get(search_language_families))
        .route("/new-language-family", get(new_language_family_form))
        .route("/language-families/{code}", get(view_language_family))
        .route(
            "/language-families/{code}/edit",
            get(edit_language_family_form),
        )
        .route(
            "/language-families/{code}/delete",
            get(delete_language_family_form),
        );

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "language_families/search.html")]
struct SearchLanguageFamiliesTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_query: SearchLanguageFamilies,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<FamilyWithContributors>>,
}

async fn search_language_families(
    s: Session,
    language_families: LanguageFamilyRepository,
    Query(query): Query<SearchLanguageFamilies>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let requestor = s.user();

    let query = SearchLanguageFamilies {
        q: query.q.and_then(|q| {
            let trimmed = q.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        owner: query.owner.and_then(|o| {
            let trimmed = o.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        has_language: query.has_language.and_then(|h| {
            let trimmed = h.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
    };

    let results = match language_families
        .search(query.clone(), pagination.clone())
        .await
    {
        Ok(res) => {
            let mut materialized_results = vec![];
            for family in res.items {
                materialized_results.push(attempt!(
                    s,
                    language_families.materialize(family, requestor).await
                ));
            }
            Some(PaginatedResponse {
                items: materialized_results,
                total: res.total,
                offset: res.offset,
                limit: res.limit,
                has_more: res.has_more,
            })
        }
        Err(e) => {
            let template = SearchLanguageFamiliesTemplate {
                current_user: s.user().cloned(),
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };

    let template = SearchLanguageFamiliesTemplate {
        current_user: s.user().cloned(),
        error: None,
        previous_query: query,
        previous_pagination: pagination,
        results,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/view.html")]
struct ViewLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    owner: User,
    contributor_count: usize,
    member_count: usize,
    rendered_description: String,
    can_edit_language_family: bool,
    can_delete_language_family: bool,
    is_liked: bool,
    pending_invite: Option<(LanguageFamilyInvite, User)>,
    family_tree_svg: String,
    json_ld: String,
}

#[allow(clippy::too_many_arguments)]
async fn view_language_family(
    s: Session,
    user_agent: Option<TypedHeader<UserAgent>>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    invites: LanguageFamilyInviteRepository,
    members: LanguageFamilyMemberRepository,
    users: UserRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let owner = attempt!(s, users.find_by_id(family.created_by).await);
    if let Some(ua) = user_agent
        && ua.as_str().to_lowercase().contains("discordbot")
    {
        return okay(
            render_embed(
                embed::EmbedTarget::Discord,
                GenericEmbed {
                    title: family.name,
                    description: format!(
                        "{}\n\n⭐️ {}",
                        truncate_description(&family.description),
                        family.like_count
                    ),
                    author: Some(owner.clone()),
                    color: if owner.gender.is_empty() { None } else { Some(owner.gender.clone()) },
                    url: format!(
                        "{}/language-families/{}",
                        &crate::CONFIG.public_url_base,
                        family.code
                    ),
                    image: None,
                },
            )
            .await
            .into_response(),
        );
    }
    let rendered_description = crate::md::render_md(&family.description).unwrap_or_default();

    // Count contributors (non-owner permissions)
    let contributor_count =
        usize::try_from(attempt!(s, permissions.count_contributors(family.id).await)).unwrap_or(0);

    let can_edit_language_family = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let can_delete_language_family = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let is_liked = if let Some(user) = s.user() {
        language_families
            .is_liked(&family.id, &user.id)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    // Check for pending invites
    let pending_invite = if let Some(user) = s.user() {
        match invites
            .find_by_family_and_recipient_unchecked(family.id, user.id)
            .await
        {
            Ok(Some(invite)) if invite.accepted_at.is_none() => {
                match users.find_by_id(invite.sender).await {
                    Ok(sender) => Some((invite, sender)),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let member_count =
        usize::try_from(attempt!(s, members.count_by_family(family.id).await)).unwrap_or(0);

    // Generate family tree SVG
    let family_tree_svg = crate::util::graph_svg::render_family_tree(&family, &members)
        .await
        .unwrap_or_default();

    let json_ld = attempt!(
        s,
        serde_json::to_string(&attempt!(s, language_families.as_json_ld(&family).await))
            .map_err(Into::into)
    );

    let template = ViewLanguageFamilyTemplate {
        current_user: s.user().cloned(),
        family,
        owner,
        contributor_count,
        rendered_description,
        can_edit_language_family,
        can_delete_language_family,
        is_liked,
        pending_invite,
        family_tree_svg,
        member_count,
        json_ld,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "language_families/new.html")]
struct NewLanguageFamilyTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
}

async fn new_language_family_form(s: Session) -> (StatusCode, Response) {
    let user = get_user!(s);

    let template = NewLanguageFamilyTemplate {
        current_user: Some(user),
        error: None,
        previous_code: String::new(),
        previous_name: String::new(),
        previous_description: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct NewLanguageFamilyFormData {
    code: String,
    name: String,
    description: String,
}

async fn new_language_family_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    Form(form): Form<NewLanguageFamilyFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);

    match language_families
        .create(
            user.clone(),
            CreateLanguageFamily {
                code: form.code.clone(),
                name: form.name.clone(),
                description: form.description.clone(),
            },
        )
        .await
    {
        Ok(family) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let template = NewLanguageFamilyTemplate {
                current_user: Some(user),
                error: Some(e),
                previous_code: form.code,
                previous_name: form.name,
                previous_description: form.description,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/edit.html")]
struct EditLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    error: Option<AppError>,
    previous_code: String,
    previous_name: String,
    previous_description: String,
    can_edit_family: bool,
    will_create_audit_log: bool,
}

async fn edit_language_family_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = EditLanguageFamilyTemplate {
        current_user: Some(user),
        family: family.clone(),
        error: None,
        previous_code: family.code,
        previous_name: family.name,
        previous_description: family.description,
        can_edit_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct EditLanguageFamilyFormData {
    code: String,
    name: String,
    description: String,
}

async fn edit_language_family_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
    Form(form): Form<EditLanguageFamilyFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_edit_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    match language_families
        .update(
            &user,
            family.id,
            crate::model::language_families::UpdateLanguageFamily {
                code: if form.code == family.code {
                    None
                } else {
                    Some(form.code.clone())
                },
                name: if form.name == family.name {
                    None
                } else {
                    Some(form.name.clone())
                },
                description: if form.description == family.description {
                    None
                } else {
                    Some(form.description.clone())
                },
            },
        )
        .await
    {
        Ok(updated) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", updated.code)).into_response(),
        ),
        Err(e) => {
            let template = EditLanguageFamilyTemplate {
                current_user: Some(user),
                family,
                error: Some(e),
                previous_code: form.code,
                previous_name: form.name,
                previous_description: form.description,
                can_edit_family,
                will_create_audit_log,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/delete.html")]
struct DeleteLanguageFamilyTemplate {
    current_user: Option<User>,
    family: LanguageFamily,
    can_delete_family: bool,
    will_create_audit_log: bool,
}

async fn delete_language_family_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let is_admin_or_mod = crate::util::is_admin_or_mod(&state, user.id)
        .await
        .unwrap_or(false);

    let can_delete_family = is_admin_or_mod
        || permissions
            .has_permission(user.id, family.id, PermissionLevel::Owner)
            .await
            .unwrap_or(false);

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let template = DeleteLanguageFamilyTemplate {
        current_user: Some(user),
        family,
        can_delete_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_language_family_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match language_families.delete(&user, family.id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to("/language-families").into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}
