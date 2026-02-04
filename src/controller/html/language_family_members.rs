use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    attempt,
    controller::html::{LanguagesWithContributors, okay, render_template},
    err::AppError,
    get_user,
    model::{
        contribution_stats::ContributionStatsRepository, language_families::{FamilyWithContributors, LanguageFamily, LanguageFamilyRepository}, language_family_members::{LanguageFamilyMemberRepository, LanguageFamilyRelationType, MemberWithLanguages, SearchLanguageFamilyMembers}, language_family_permissions::LanguageFamilyPermissionRepository, language_invites::PermissionLevel, language_permissions::LanguagePermissionRepository, languages::{Language, LanguageRepository}, users::User
    },
    pagination::{PaginatedRequest, PaginatedResponse},
    util::{AppState, extract_session::Session},
};

pub fn create_router() -> Router<AppState> {
    Router::<AppState>::new()
        .route("/languages/{code}/relatives", get(search_relatives))
        .route("/language-families/{code}/add-root", post(add_root_submit))
        .route("/language-families/{code}/members/{id}", get(view_member))
        .route("/language-families/{code}/members/{id}/add-language-member", post(add_language_member_submit))
        .route("/language-families/{code}/members/{id}/add-grouping", post(add_grouping_submit))
        .route("/language-families/{code}/members/{id}/delete", post(delete_member_submit))
        .route("/language-families/{code}/members", get(search_members))
        .route("/language-families/{code}/add-root", get(add_root_form))
        .route("/language-families/{code}/members/{id}/add-child", get(add_child_form))
        .route("/language-families/{code}/members/{id}/add-language-member", get(add_language_member_form))
        .route("/language-families/{code}/members/{id}/add-grouping", get(add_grouping_form))
        .route("/language-families/{code}/members/{id}/delete", get(delete_member_form))
}

#[derive(Template)]
#[template(path = "language_families/members/view.html")]
struct ViewMemberTemplate {
    name: String,
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    can_edit_family: bool,
    can_edit_language: bool,
    language: Option<LanguagesWithContributors>,
    parent_member: Option<MemberWithLanguages>,
    children: Vec<MemberWithLanguages>,
}

async fn view_member(
    State(state): State<AppState>,
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    contribution_stats: ContributionStatsRepository,
    languages: LanguageRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member = attempt!(s, members.find_by_id(id).await);
    let member = attempt!(s, members.materialize(member).await);

    let can_edit_family = if let Some(user) = s.user() {
        attempt!(
            s,
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
        )
    } else {
        false
    };

    let language = if let Some(language) = &member.language {
        let top_contributors = attempt!(
            s,
            contribution_stats
                .get_top_contributors(&language.id, 5)
                .await
        );
        let is_liked = if let Some(user) = s.user() { attempt!(s, languages.is_liked(&user.id, &language.id).await) } else { false };
        let language_with_contributors = LanguagesWithContributors {
            language: language.clone(),
            top_contributors,
            is_liked,
        };

        Some(language_with_contributors)
    } else {
        None
    };

    let can_edit_language = if let (Some(user), Some(language)) = (s.user(), &member.language) {
        LanguagePermissionRepository::new(state.clone())
            .has_permission(user.id, language.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let parent_member = if let Some(parent_id) = member.member.parent_member_id {
        let parent_raw = attempt!(s, members.find_by_id(parent_id).await);
        Some(attempt!(s, members.materialize(parent_raw).await))
    } else {
        None
    };

    let name = member.name();
    let get_five = PaginatedRequest { limit: 5, offset: 0 };

    let children = attempt!(
        s,
        members
            .search(
                SearchLanguageFamilyMembers {
                    family_code: Some(code.clone()),
                    q: None,
                    parent_language_code: None,
                    parent_member_id: Some(member.member.id),
                    language_code: None,
                    relation_type: None,
                },
                get_five,
            )
            .await
    );
    let mut materialized_children = Vec::new();
    for child in children.items {
        if let Ok(m) = members.materialize(child).await {
            materialized_children.push(m);
        }
    }

    let family = attempt!(s, language_families.materialize(family, s.user()).await);

    let template = ViewMemberTemplate {
        name,
        current_user: s.user().cloned(),
        family,
        member,
        can_edit_family,
        language,
        can_edit_language,
        parent_member,
        children: materialized_children,
    };

    okay(render_template(template))
}


#[derive(Template)]
#[template(path = "language_families/members/search.html")]
#[allow(dead_code)]
struct SearchMembersTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    error: Option<AppError>,
    previous_query: SearchLanguageFamilyMembers,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<MemberWithLanguages>>,
    previous_search: String,
    can_edit_family: bool,
}

async fn search_members(
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
    Query(query): Query<SearchLanguageFamilyMembers>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit_family = if let Some(user) = s.user() {
        permissions
            .has_permission(user.id, family.id, PermissionLevel::Editor)
            .await
            .unwrap_or(false)
    } else {
        false
    };

    let search_query = SearchLanguageFamilyMembers {
        family_code: Some(code.clone()),
        ..query.clone()
    };

    let results = match members.search(search_query, pagination.clone()).await {
        Ok(res) => {
            // Materialize all members
            let mut materialized = Vec::new();
            for member in res.items {
                if let Ok(m) = members.materialize(member).await {
                    materialized.push(m);
                }
            }
            Some(PaginatedResponse {
                items: materialized,
                total: res.total,
                limit: res.limit,
                offset: res.offset,
                has_more: res.has_more,
            })
        }
        Err(e) => {
            let family = attempt!(s, language_families.materialize(family, s.user()).await);
            let template = SearchMembersTemplate {
                current_user: s.user().cloned(),
                family,
                error: Some(e),
                previous_query: query,
                previous_pagination: pagination,
                results: None,
                previous_search: String::new(),
                can_edit_family,
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };
    let family = attempt!(s, language_families.materialize(family, s.user()).await);

    let template = SearchMembersTemplate {
        current_user: s.user().cloned(),
        family,
        error: None,
        previous_query: query.clone(),
        previous_pagination: pagination,
        results,
        previous_search: query.q.unwrap_or_default(),
        can_edit_family,
    };

    okay(render_template(template))
}

#[derive(Template)]
#[template(path = "languages/relatives.html")]
#[allow(dead_code)]
struct SearchRelativesTemplate {
    current_user: Option<User>,
    error: Option<AppError>,
    language: LanguagesWithContributors,
    previous_query: SearchLanguageFamilyMembers,
    previous_pagination: PaginatedRequest,
    results: Option<PaginatedResponse<MemberWithLanguages>>,
    previous_search: String,
}

async fn search_relatives(
    s: Session,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    Path(code): Path<String>,
    Query(query): Query<SearchLanguageFamilyMembers>,
    Query(pagination): Query<PaginatedRequest>,
) -> (StatusCode, Response) {
    let language = attempt!(s, languages.find_by_code(&code).await);
    let language = attempt!(s, languages.materialize(language, s.user()).await);

    let search_query = SearchLanguageFamilyMembers {
        language_code: Some(code.clone()),
        ..query.clone()
    };

    let results = match members.search(search_query, pagination.clone()).await {
        Ok(res) => {
            // Materialize all members
            let mut materialized = Vec::new();
            for member in res.items {
                if let Ok(m) = members.materialize(member).await {
                    materialized.push(m);
                }
            }
            Some(PaginatedResponse {
                items: materialized,
                total: res.total,
                limit: res.limit,
                offset: res.offset,
                has_more: res.has_more,
            })
        }
        Err(e) => {
            let template = SearchRelativesTemplate {
                current_user: s.user().cloned(),
                error: Some(e),
                language,
                previous_query: query,
                previous_pagination: pagination,
                results: None,
                previous_search: String::new(),
            };
            return (StatusCode::BAD_REQUEST, render_template(template));
        }
    };
    let template = SearchRelativesTemplate {
        current_user: s.user().cloned(),
        error: None,
        language,
        previous_query: query.clone(),
        previous_pagination: pagination,
        results,
        previous_search: query.q.unwrap_or_default(),
    };

    okay(render_template(template))
}




#[derive(Template)]
#[template(path = "language_families/members/add-root.html")]
struct AddRootTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    error: Option<AppError>,
    will_create_audit_log: bool,
    available_languages: Vec<Language>,
    previous_language_code: String,
}

async fn add_root_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path(code): Path<String>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    let can_edit = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    // Get languages the user can add (ones they have editor permission on)
    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddRootTemplate {
        current_user: Some(user),
        family,
        error: None,
        will_create_audit_log,
        available_languages,
        previous_language_code: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddRootFormData {
    language_code: String,
}

async fn add_root_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    Path(code): Path<String>,
    Form(form): Form<AddRootFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members
        .create(
            user.clone(),
            family.clone(),
            None, // No parent for root
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: form.language_code.clone(),
                relation_type: LanguageFamilyRelationType::Descendant,
                notes: None,
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let available_languages = languages.list_editable_by_user(user.id).await.unwrap_or_default();
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddRootTemplate {
                current_user: Some(user),
                family,
                error: Some(e),
                will_create_audit_log,
                available_languages,
                previous_language_code: form.language_code,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}


#[derive(Template)]
#[template(path = "language_families/members/add.html")]
#[allow(dead_code)]
struct AddChildTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    can_edit_family: bool,
}

async fn add_child_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddChildTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        can_edit_family,
    };

    okay(render_template(template))
}


#[derive(Template)]
#[template(path = "language_families/members/add-language-member.html")]
#[allow(dead_code)]
struct AddLanguageMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    available_languages: Vec<Language>,
    previous_language_code: String,
    previous_is_hybrid: Option<bool>,
    previous_notes: String,
    can_edit_family: bool,
}

async fn add_language_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let available_languages = attempt!(s, languages.list_editable_by_user(user.id).await);
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddLanguageMemberTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        available_languages,
        previous_language_code: String::new(),
        previous_is_hybrid: None,
        previous_notes: String::new(),
        can_edit_family,
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddLanguageMemberFormData {
    language_code: String,
    #[serde(default)]
    is_hybrid: Option<String>,
    notes: String,
}

#[allow(clippy::too_many_arguments)]
async fn add_language_member_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    languages: LanguageRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<AddLanguageMemberFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let is_hybrid = form.is_hybrid.as_ref().is_some_and(|v| v == "true");
    let relation_type = if is_hybrid {
        LanguageFamilyRelationType::Hybrid
    } else {
        LanguageFamilyRelationType::Descendant
    };

    match members
        .create(
            user.clone(),
            family.clone(),
            Some(member_id),
            crate::model::language_family_members::CreateLanguageFamilyMember {
                language_code: form.language_code.clone(),
                relation_type,
                notes: if form.notes.is_empty() { None } else { Some(form.notes.clone()) },
            },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let can_edit_family = permissions
                .has_permission(user.id, family.id, PermissionLevel::Editor)
                .await
                .unwrap_or(false);
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let available_languages = languages.list_editable_by_user(user.id).await.unwrap_or_default();
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddLanguageMemberTemplate {
                current_user: Some(user),
                family,
                parent_member,
                error: Some(e),
                will_create_audit_log,
                available_languages,
                previous_language_code: form.language_code,
                previous_is_hybrid: Some(is_hybrid),
                previous_notes: form.notes,
                can_edit_family,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}


#[derive(Template)]
#[template(path = "language_families/members/add-grouping.html")]
struct AddGroupingTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    parent_member: MemberWithLanguages,
    error: Option<AppError>,
    will_create_audit_log: bool,
    previous_notes: String,
}

async fn add_grouping_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to add members to this family"),
        )
        .await;
    }

    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;

    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = AddGroupingTemplate {
        current_user: Some(user),
        family,
        parent_member,
        error: None,
        will_create_audit_log,
        previous_notes: String::new(),
    };

    okay(render_template(template))
}

#[derive(Deserialize)]
struct AddGroupingFormData {
    notes: String,
}

async fn add_grouping_submit(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
    Form(form): Form<AddGroupingFormData>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let parent_member_raw = attempt!(s, members.find_by_id(member_id).await);
    let parent_member = attempt!(s, members.materialize(parent_member_raw).await);

    match members
        .create_grouping(
            user.clone(),
            family.clone(),
            Some(member_id),
            if form.notes.is_empty() { None } else { Some(form.notes.clone()) },
        )
        .await
    {
        Ok(_) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => {
            let will_create_audit_log =
                crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
            let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

            let template = AddGroupingTemplate {
                current_user: Some(user),
                family,
                parent_member,
                error: Some(e),
                will_create_audit_log,
                previous_notes: form.notes,
            };
            (StatusCode::BAD_REQUEST, render_template(template))
        }
    }
}

#[derive(Template)]
#[template(path = "language_families/members/delete.html")]
struct DeleteMemberTemplate {
    current_user: Option<User>,
    family: FamilyWithContributors,
    member: MemberWithLanguages,
    can_edit_family: bool,
    will_create_audit_log: bool,
}

async fn delete_member_form(
    s: Session,
    State(state): State<AppState>,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    permissions: LanguageFamilyPermissionRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);
    let member_raw = attempt!(s, members.find_by_id(member_id).await);
    let member = attempt!(s, members.materialize(member_raw).await);

    let can_edit_family = permissions
        .has_permission(user.id, family.id, PermissionLevel::Editor)
        .await
        .unwrap_or(false);

    if !can_edit_family {
        return crate::controller::html::render_generic_error(
            s,
            crate::err::forbidden("You don't have permission to delete members from this family"),
        )
        .await;
    }


    let will_create_audit_log =
        crate::util::will_create_audit_log_for_family(&state, &user, family.id).await;
    let family = attempt!(s, language_families.materialize(family, Some(&user)).await);

    let template = DeleteMemberTemplate {
        current_user: Some(user),
        family,
        member,
        can_edit_family,
        will_create_audit_log,
    };

    okay(render_template(template))
}

async fn delete_member_submit(
    s: Session,
    language_families: LanguageFamilyRepository,
    members: LanguageFamilyMemberRepository,
    Path((code, member_id)): Path<(String, Uuid)>,
) -> (StatusCode, Response) {
    let user = get_user!(s);
    let family = attempt!(s, language_families.find_by_code(&code).await);

    match members.delete(&user, member_id).await {
        Ok(()) => (
            StatusCode::SEE_OTHER,
            Redirect::to(&format!("/language-families/{}", family.code)).into_response(),
        ),
        Err(e) => crate::controller::html::render_generic_error(s, e).await,
    }
}
