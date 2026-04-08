pub mod audit_log;
pub mod bookmarks;
pub mod contribution_stats;
pub mod definitions;
pub mod email_verification_tokens;
pub mod language_families;
pub mod language_family_invites;
pub mod language_family_members;
pub mod language_family_permissions;
pub mod language_invites;
pub mod language_permissions;
pub mod languages;
pub mod password_reset_tokens;
pub mod phonology_tables;
pub mod quotation_suggestions;
pub mod quotations;
pub mod reports;
pub mod sessions;
pub mod sound_change_sets;
pub mod translatable;
pub mod translations;
pub mod user_activities;
pub mod user_bans;
pub mod user_tags;
pub mod users;
pub mod word_classes;
pub mod word_relations;
pub mod words;

// pub trait Resource {
//     type Repository;
//     type Materialized;
// }

// pub trait Repository<R: Resource> {
//     fn new(state: AppState) -> Self;

//     async fn get_by_id(&self, uuid: Uuid) -> Option<R>;

//     async fn materialize(&self, resource: R, requestor: Option<&users::User>) -> R::Materialized;
// }
