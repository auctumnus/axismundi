# Axis Mundi API

All endpoints are served below `/api`. This reference is generated from the
routes registered in `src/controller/api`.

## Conventions

### Authentication

Use either `Authorization: Bearer <token>` or the `session` cookie created by
`POST /sessions`. “Auth” in the tables requires a valid session; the server
also enforces the stated ownership, editor, moderator, or administrator role.
Some writes also require a verified email address.

### Pagination and errors

List endpoints use `limit` and `offset`. The default limit is 10; it must be
1–100, and offset must be non-negative.

```json
{ "items": [], "total": 0, "offset": 0, "limit": 10, "has_more": false }
```

Timestamps are RFC 3339 UTC strings and IDs are UUIDs unless noted. Errors have
plain-text bodies, not JSON envelopes. Deletes return `204 No Content` unless
noted; `DELETE /reports/{id}` is the exception and returns `200` with JSON
`null`.

## Request bodies

Fields marked optional may be omitted. Update requests change only supplied
fields.

| Resource | Create | Update |
| --- | --- | --- |
| User | `username`, `email`, `password`, optional `display_name`, `description`, `pronouns`, `gender` | optional `username`, `email`, `display_name`, `description`, `pronouns`, `gender`, `current_password`, `new_password` |
| Language | `code`, `name`, optional `private` (false by default), `description` (empty by default) | optional `code`, `name`, `private`, `description` |
| Word class/category | `name`, `abbreviation`, optional `notes` | optional `name`, `abbreviation`, `notes` |
| Word | `word`, `word_class`, optional `ipa`, `notes`, `extra`, `categories`, `definitions` | optional `word`, `word_class`, `ipa`, `notes`, `extra`, `categories` |
| Definition | `definition`, optional `context` | optional `definition`, `context` |
| Translatable | `title`, `english`, optional `source_name`, `source_url`, `source_content`, `source_language`, `description`, `as_draft` | optional `title`, `english`, `source_name`, `source_url`, `source_content`, `source_language`, `description` |
| Translation | `translated_text`, optional `translated_title`, `ipa`, `gloss`, `notes` | optional `translated_text`, `translated_title`, `ipa`, `gloss`, `notes` |
| Quotation | `definition`, `span_start`, `span_end`, optional `highlight_start`, `highlight_end`, `notes` | optional `span_start`, `span_end`, `highlight_start`, `highlight_end`, `notes` |
| Language family | `code`, `name`, `description` | No update route |
| Family member | required `relation_type`; optional `language_code`, `title`, `notes` | No update route |
| Phonology table | `name`, optional `description`, required `body` | optional `name`, `description`, `body` |
| Sound-change set | `name`, `description`, `changes` | optional `name`, `description`, `changes` |
| News | `title`, `content`, optional `as_draft` | optional `title`, `content` |
| Report | `resource_type`, `resource_id`, `reason` | moderator-only: optional `priority`, `resolution_status`, `resolution_note`, `resolution_status_hidden`, `resolution_note_hidden` |

A word's optional `definitions` is an ordered array of definition-create
bodies. Word responses include its category references along with fields such
as `word`, `slug`, `lemma`, `ipa`, `notes`, `extra`, `like_count`,
`bookmark`, `language_code`, and `word_class_abbreviation`.

## Health, sessions, and users

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/health` | No | Health check; not rate limited. |
| POST | `/sessions` | No | Body: `email`, `password`. Returns `token`, `expires_at`, and sets a cookie. |
| GET | `/sessions` | Yes | Lists the current user's sessions. |
| POST | `/users` | No | Creates a user. Returns `user` and `resend_token`. |
| GET | `/users` | No | Paginated search: `q`, `created_before`, `created_after`, `verified`. |
| GET | `/users/{username}` | No | Gets a public profile. |
| PUT | `/users/{username}` | Yes | Updates that user. |
| PUT | `/users/{username}/profile-picture` | Yes | `multipart/form-data` with an `image` part. |
| POST | `/verify/{id}` | No | Body: `token`, `email`. |
| POST | `/resend-verification/{id}` | No | Resends a verification email. |
| POST | `/reset-password/start` | No | Body: `email`. |
| POST | `/reset-password/complete` | No | Body: `uuid`, `token`, `new_password`. |
| GET | `/users/{username}/activities` | No | Paginated activity history. |
| DELETE | `/activities/{id}` | Auth | Deletes an activity when the caller has permission. |
| GET / POST | `/users/{username}/tags` | No / moderator-admin | List tags; create body is `tag`, optional `hidden`. |
| DELETE | `/users/{username}/tags/{tag}` | Moderator/admin | Deletes a tag. |

A public user exposes `username`, `display_name`, `description`,
`pronouns`, `gender`, `bookmark`, `profile_picture_url`, `banner_url`,
`tags`, `created_at`, and `updated_at`; it does not expose IDs, email
addresses, password hashes, or verification state.

## Bookmarks and moderation

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/bookmarks/{slug}` | No | Redirects to the current resource URL. |
| GET / POST | `/bans` | No / moderator-admin | Paginated search: `text_query`, `banned_by`; create body: `user_id`, `reason`. |
| GET / DELETE | `/bans/{username}` | No / moderator-admin | Gets or removes a ban. |
| GET / POST | `/reports` | Moderator/admin / Auth | Search supports `text_query`, `resource_type`, `resource_id`, `reporter`, `resolution_status`, `priority`; POST creates a report. |
| GET | `/reports/own` | Yes | Paginated reports created by the caller. |
| GET / PATCH / DELETE | `/reports/{id}` | Yes / moderator-admin / admin | Gets, moderates, or deletes a report. |
| GET | `/audit_logs` | Moderator/admin | Paginated search: `user_id`, `action`, `resource_type`, `resource_id`. |
| GET | `/audit_logs/{id}` | Moderator/admin | Gets an audit entry. |

## Languages and invitations

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET / POST | `/languages` | No / Auth | List query: `q`, `owned_by`, `edited_by`, `created_before`, `created_after`, `in_family`; creates a language. |
| GET / PUT / DELETE | `/languages/{code}` | No / language editor / language owner | Gets, updates, or deletes a language. |
| GET | `/languages/{code}/owner` | No | Redirects to `/users/{username}`. |
| GET | `/languages/{code}/editors` | No | Paginated editors. |
| POST | `/languages/{code}/like` | Auth | Likes a language; returns `like_count`. |
| POST | `/languages/{code}/unlike` | Auth | Removes the like; returns `like_count`. |
| GET | `/languages/{code}/activities` | No | Paginated activity history. |
| GET | `/languages/{code}/permissions` | Language editor | Lists permission assignments. |
| GET | `/languages/{code}/permissions/{username}` | Language editor | Gets one assignment. |
| GET | `/languages/{code}/invites` | Language editor | Paginated search: `sender`, `recipient`, `created_before`, `created_after`, `accepted_before`, `accepted_after`. |
| POST | `/languages/{code}/invites/{username}` | Language admin/owner | Body: `permission_level`. |
| GET / DELETE | `/languages/{code}/invites/{username}` | Participant/editor / participant-admin-owner | Gets, deletes, or rejects an invite. |
| POST | `/languages/{code}/accept-invite` | Yes | Accepts the caller's invite. |

Language-permission routes are currently read-only: there is no registered
`PUT` or `DELETE /languages/{code}/permissions/{username}` endpoint.

## Vocabulary

Word-class and word-category list searches accept `text_query`,
`created_before`, `created_after`, `created_by`, and `updated_by`, plus
pagination.

| Method | Path | Auth |
| --- | --- | --- |
| GET / POST | `/languages/{code}/word-classes` | No / language editor |
| GET / PUT / DELETE | `/languages/{code}/word-classes/{abbreviation}` | No / language editor / language editor |
| GET / POST | `/languages/{code}/word-categories` | No / language editor |
| GET / PUT / DELETE | `/languages/{code}/word-categories/{abbreviation}` | No / language editor / language editor |

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/words/search` | Auth | Cross-language search: required `q`, optional `exclude_id`, `limit`. |
| GET / POST | `/languages/{code}/words` | No / language editor | Search: `q`, `exact_slug`, `word_class`, `created_before`, `created_after`, `categories[]`; create accepts categories and definitions. |
| GET / PUT / DELETE | `/languages/{code}/words/{slug}/{lemma}` | No / language editor / language editor | Gets, updates, or deletes a word. |
| POST | `/languages/{code}/words/{slug}/{lemma}/like` | Auth | Returns updated `like_count`. |
| POST | `/languages/{code}/words/{slug}/{lemma}/unlike` | Auth | Returns updated `like_count`. |
| GET / POST | `/languages/{code}/words/{slug}/{lemma}/definitions` | No / language editor | Lists or creates definitions. |
| POST | `/languages/{code}/words/{slug}/{lemma}/definitions/swap` | Language editor | Body: `id1`, `id2`; swaps positions. |
| GET / PUT / DELETE | `/languages/{code}/words/{slug}/{lemma}/definitions/{id}` | No / language editor / language editor | Gets, updates, or deletes a definition. |
| GET / POST | `/languages/{code}/words/{slug}/{lemma}/relations` | No / language editor | Search: `direction`, `kind`; create body: `kind`, `language`, `slug`, `lemma`. |
| DELETE | `/languages/{code}/words/{slug}/{lemma}/relations/{related_code}/{related_slug}/{related_lemma}` | Language editor | Deletes a relation. |
| GET | `/languages/{code}/words/{slug}/{lemma}/etymology` | No | Returns etymology/cognacy data. |
| GET | `/languages/{code}/words/{slug}/{lemma}/etymology.svg` | No | Returns an SVG graph. |

## Translatables, translations, and quotations

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET / POST | `/translatable` | No / Auth | Paginated search: `q`, `draft_status`; any verified, unbanned user can create a translatable. |
| GET / PUT / DELETE | `/translatable/{slug}` | No / creator / creator | Gets, updates, or deletes a translatable. |
| POST | `/translatable/{slug}/like` | Auth | Returns `like_count`. |
| POST | `/translatable/{slug}/unlike` | Auth | Returns `like_count`. |
| GET | `/translatable/{translatable_slug}/translations` | No | Paginated translations for a translatable. |
| POST | `/translatable/{translatable_slug}/translations/{code}` | Language editor | Creates a translation. |
| GET | `/languages/{code}/translations` | No | Paginated translations in a language. |
| GET / PUT / DELETE | `/translatable/{translatable_slug}/translations/{code}` | No / language editor / language editor | Gets, updates, or deletes a translation. |
| POST | `/translatable/{translatable_slug}/translations/{code}/like` | Auth | Returns `like_count`. |
| POST | `/translatable/{translatable_slug}/translations/{code}/unlike` | Auth | Returns `like_count`. |
| GET / POST | `/translatable/{translatable_slug}/translations/{language_code}/quotations` | No / language editor | Lists or creates quotations. |
| GET / PUT / DELETE | `/translatable/{translatable_slug}/translations/{language_code}/quotations/{id}` | No / language editor / language editor | Gets, updates, or deletes a quotation. |
| GET | `/languages/{language_code}/words/{word_slug}/definitions/{definition_id}/quotations` | No | Paginated quotations for a definition. This route uses only word slug, not lemma. |
| GET / POST | `/languages/{code}/quotation-suggestions` | No / Auth | List query requires `content`; create body is `span_content`, `definition`. |
| DELETE | `/languages/{code}/quotation-suggestions/{id}` | Auth | Deletes a suggestion. |

## Language families

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET / POST | `/language-families` | No / Auth | Search: `q`, `owner`, `has_language`; creates a family. |
| GET | `/language-families/{code}` | No | Gets a family. |
| POST | `/language-families/{code}/like` | Auth | Returns `like_count`. |
| POST | `/language-families/{code}/unlike` | Auth | Returns `like_count`. |
| GET | `/language-families/{code}/tree.svg` | No | Returns an SVG family tree. |
| GET | `/language-families/{code}/contributors` | No | Paginated contributors. |
| GET | `/language-families/{code}/permissions` | Family editor | Lists family permissions. |
| GET | `/language-families/{code}/permissions/{username}` | Family editor | Gets one assignment. |
| GET | `/language-families/{code}/invites` | Family editor | Paginated invitation search. |
| POST | `/language-families/{code}/invites/{username}` | Family admin/owner | Body: `permission_level`. |
| GET / DELETE | `/language-families/{code}/invites/{username}` | Participant/editor / participant-admin-owner | Gets or removes an invite. |
| POST | `/language-families/{code}/accept-invite` | Yes | Accepts the caller's invite. |

Family-permission routes are read-only: no family-permission mutation endpoints
are currently registered.

### Family members

These endpoints intentionally use singular `/language-family` paths. Member
searches accept `family_code`, `parent_language_code`, `parent_member_id`,
`language_code`, `relation_type`, and `q`, plus pagination.

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/language-family-members` | No | Global paginated search. |
| GET / POST | `/language-family/{code}/members` | No / family editor | Lists or creates a root-level member. |
| GET | `/language-family/{code}/root` | No | Gets the root member. |
| GET / DELETE | `/language-family/{code}/members/by-id/{id}` | No / family editor | Gets or deletes a member. |
| GET / POST | `/language-family/{code}/members/by-id/{id}/children` | No / family editor | Lists or creates child members. |
| GET / DELETE | `/language-family/{code}/members/by-code/{code}` | No / family editor | Gets or deletes a language's member; the second `{code}` is the language code. |
| GET / POST | `/language-family/{code}/members/by-code/{code}/children` | No / family editor | Lists or creates child members; the second `{code}` is the language code. |

## Phonology tables and sound changes

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET / POST | `/languages/{code}/phonology-tables` | No / language editor | Search: `q`, `created_before`, `created_after`; lists or creates tables. |
| POST | `/languages/{code}/phonology-tables/swap` | Language editor | Body: `id1`, `id2`; swaps table positions. |
| GET / PUT / DELETE | `/languages/{code}/phonology-tables/{id}` | No / language editor / language editor | Gets, updates, or deletes a table. |
| GET / POST | `/languages/{code}/sound-change-sets` | No / language editor | Search: `q`, `author`; lists or creates sets. |
| GET / PUT / DELETE | `/languages/{code}/sound-change-sets/{id}` | No / language editor / language editor | Gets, updates, or deletes a set. |
| POST | `/sound-change-sets/run` | No | Runs a supplied Lexurgy set: `changes`, `inputWords`, optional `traceWords`, `startAt`, `stopBefore`, `allowPolling`. |
| POST | `/sound-change-sets/{id}/run` | No | Runs a stored set. Body: `input_words` (array of strings). |

The direct run endpoint uses camelCase names because it forwards the Lexurgy
request payload; it is not a sound-change-set creation payload.

## News

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET / POST | `/news` | No / moderator-admin | Paginated search: `q`, `draft_status`; creates an article or draft. |
| GET / PUT / DELETE | `/news/{slug}` | No / moderator-admin / moderator-admin | Gets, updates, or deletes an article. Draft visibility requires staff access. |
| POST | `/news/{slug}/publish` | Moderator/admin | Publishes an article. |
| POST | `/news/{slug}/unpublish` | Moderator/admin | Returns an article to draft status. |
