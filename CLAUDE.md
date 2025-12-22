all sql is in postgres 18

when you write sql, you should consult the tables @migrations

always use `just test` for tests. if you need to do something more complicated than what i have in the justfile you should ask about it

sometimes the db explodes and i get a bunch of errors in my ide and i think you get told about them but you can ignore the sqlx failing to connect to database queries its fine

writing code off to the side is pawbably not a reasonable debugging strategy

axum uses {id} style route parameters, not :id style

html for pages goes in templates, code for them goes into src/controllers/html

when you need to test changes to the html controllers, don't bother running `just test`, do `just build`

"secure routes" means "has user security implications", not "needs authentication"

the frontend has typescript stored in `frontend/src`; that's also where the css and such are


# Hacking Guide for Axismundi

This guide is for future Claude instances (or humans) working on this codebase. It covers patterns, conventions, and step-by-step instructions for common tasks.

## Quick Orientation

**Tech Stack:**
- Backend: Rust + Axum web framework + SQLx (PostgreSQL 18)
- Frontend: TypeScript + Alpine.js + SWC
- Templates: Askama (server-side HTML rendering)
- Infrastructure: Docker Compose (Postgres + MinIO S3)

**Key Commands:**
```bash
just dev          # Start dev servers (backend + frontend watch mode)
just test         # Run tests (uses fresh db/minio containers)
just build        # Build backend (faster than just test for HTML changes)
just build-frontend  # Build TypeScript/CSS
just db           # Start PostgreSQL container
```

**Directory Structure:**
```
src/
  controller/     # HTTP handlers
    api/          # JSON API routes
    html/         # Server-rendered HTML routes
  model/          # Data models + database repositories
  util/           # Helpers (auth, session, hashing, S3)
templates/        # Askama HTML templates
frontend/src/     # TypeScript + CSS
migrations/       # PostgreSQL migrations (numbered 001+)
```

## Common Tasks

### Adding a New Database-Backed Resource

Example: Adding a "Quotations" feature

#### 1. Create the migration

Create `migrations/014_quotations.sql`:

```sql
create table quotations (
    id uuid primary key default uuidv7(),
    text text not null,
    author_name text,
    source text,
    language_id uuid not null references languages(id) on delete cascade,
    created_by uuid not null references users(id) on delete set null,
    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp
);

create index quotations_language_id_idx on quotations(language_id);
create index quotations_created_by_idx on quotations(created_by);
```

**Migration Conventions:**
- Use `uuid primary key default uuidv7()` for IDs
- Always include `created_at`, `updated_at`, `created_by` fields
- Use `timestamp with time zone` for timestamps
- Add `on delete cascade` or `on delete set null` as appropriate
- Create indexes on foreign keys and frequently-queried columns
- Number files sequentially (001, 002, ..., 014)

#### 2. Create the model

Create `src/model/quotations.rs`:

```rust
use sqlx::FromRow;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;
use crate::err::AppResult;
use crate::AppState;
use crate::model::users::User;

// Main data struct - matches database table
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Quotation {
    pub id: Uuid,
    pub text: String,
    pub author_name: Option<String>,
    pub source: Option<String>,
    pub language_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Create request - used for POST
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateQuotation {
    #[validate(length(min = 1, max = 10000))]
    pub text: String,
    #[validate(length(max = 500))]
    pub author_name: Option<String>,
    #[validate(length(max = 1000))]
    pub source: Option<String>,
    pub language_id: Uuid,
}

// Update request - used for PATCH (all fields optional)
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateQuotation {
    #[validate(length(min = 1, max = 10000))]
    pub text: Option<String>,
    #[validate(length(max = 500))]
    pub author_name: Option<String>,
    #[validate(length(max = 1000))]
    pub source: Option<String>,
}

// Repository - handles all database operations
pub struct QuotationRepository {
    state: AppState,
}

impl QuotationRepository {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn create(&self, user: &User, req: CreateQuotation) -> AppResult<Quotation> {
        req.validate()?;

        let quotation = sqlx::query_as!(
            Quotation,
            r#"
            insert into quotations (text, author_name, source, language_id, created_by)
            values ($1, $2, $3, $4, $5)
            returning *
            "#,
            req.text,
            req.author_name,
            req.source,
            req.language_id,
            user.id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(quotation)
    }

    pub async fn find_by_id(&self, id: Uuid) -> AppResult<Quotation> {
        let quotation = sqlx::query_as!(
            Quotation,
            "select * from quotations where id = $1",
            id
        )
        .fetch_optional(&self.state.pool)
        .await?
        .ok_or_else(|| crate::err::not_found("Quotation not found"))?;

        Ok(quotation)
    }

    pub async fn list_by_language(&self, language_id: Uuid) -> AppResult<Vec<Quotation>> {
        let quotations = sqlx::query_as!(
            Quotation,
            "select * from quotations where language_id = $1 order by created_at desc",
            language_id
        )
        .fetch_all(&self.state.pool)
        .await?;

        Ok(quotations)
    }

    pub async fn update(&self, id: Uuid, req: UpdateQuotation) -> AppResult<Quotation> {
        req.validate()?;

        let mut quotation = self.find_by_id(id).await?;

        if let Some(text) = req.text {
            quotation.text = text;
        }
        if let Some(author_name) = req.author_name {
            quotation.author_name = Some(author_name);
        }
        if let Some(source) = req.source {
            quotation.source = Some(source);
        }

        let quotation = sqlx::query_as!(
            Quotation,
            r#"
            update quotations
            set text = $1, author_name = $2, source = $3, updated_at = current_timestamp
            where id = $4
            returning *
            "#,
            quotation.text,
            quotation.author_name,
            quotation.source,
            id
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(quotation)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query!("delete from quotations where id = $1", id)
            .execute(&self.state.pool)
            .await?;
        Ok(())
    }
}
```

**Model Conventions:**
- Main struct derives `FromRow, Serialize, Deserialize`
- Request structs use `#[validate(...)]` for validation
- Repository has `new(state: AppState)` constructor
- All DB operations are async and return `AppResult<T>`
- Use `sqlx::query!` or `sqlx::query_as!` for compile-time checked SQL
- Check migrations (`@migrations`) for exact column names/types

#### 3. Register the model

Add to `src/model/mod.rs`:
```rust
pub mod quotations;
```

#### 4. Create API controller

Create `src/controller/api/quotations.rs`:

```rust
use axum::{
    extract::Path,
    routing::{get, post, patch, delete},
    Json, Router,
};
use uuid::Uuid;

use crate::err::{AppResult, ensure_verified};
use crate::model::quotations::{CreateQuotation, QuotationRepository, UpdateQuotation};
use crate::util::extract_session::Session;
use crate::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/quotations", post(create_quotation))
        .route("/quotations/{id}", get(get_quotation))
        .route("/quotations/{id}", patch(update_quotation))
        .route("/quotations/{id}", delete(delete_quotation))
}

async fn create_quotation(
    s: Session,
    quotations: QuotationRepository,
    Json(req): Json<CreateQuotation>,
) -> AppResult<Json<crate::model::quotations::Quotation>> {
    let user = ensure_verified(&s)?;
    let quotation = quotations.create(user, req).await?;
    Ok(Json(quotation))
}

async fn get_quotation(
    Path(id): Path<Uuid>,
    quotations: QuotationRepository,
) -> AppResult<Json<crate::model::quotations::Quotation>> {
    let quotation = quotations.find_by_id(id).await?;
    Ok(Json(quotation))
}

async fn update_quotation(
    s: Session,
    Path(id): Path<Uuid>,
    quotations: QuotationRepository,
    Json(req): Json<UpdateQuotation>,
) -> AppResult<Json<crate::model::quotations::Quotation>> {
    let user = ensure_verified(&s)?;
    let existing = quotations.find_by_id(id).await?;

    // Check if user owns this quotation
    if existing.created_by != user.id {
        return Err(crate::err::forbidden("You can only edit your own quotations"));
    }

    let quotation = quotations.update(id, req).await?;
    Ok(Json(quotation))
}

async fn delete_quotation(
    s: Session,
    Path(id): Path<Uuid>,
    quotations: QuotationRepository,
) -> AppResult<Json<()>> {
    let user = ensure_verified(&s)?;
    let existing = quotations.find_by_id(id).await?;

    if existing.created_by != user.id {
        return Err(crate::err::forbidden("You can only delete your own quotations"));
    }

    quotations.delete(id).await?;
    Ok(Json(()))
}
```

**API Controller Conventions:**
- Export a `create_router() -> Router<AppState>` function
- Use extractors: `Session`, `Path`, `Json`, `Query`
- Return `AppResult<Json<T>>` for JSON responses
- Use `ensure_verified(&s)?` to require authenticated + verified user
- Check permissions before allowing operations
- Axum uses `{id}` style route params, NOT `:id`

Register in `src/controller/api/mod.rs`:
```rust
mod quotations;

// In create_router():
.merge(quotations::create_router())
```

#### 5. Create HTML controller

Create `src/controller/html/quotations.rs`:

```rust
use askama::Template;
use axum::{
    extract::Path,
    response::Response,
    routing::{get, post},
    Form, Router,
};
use http::StatusCode;
use uuid::Uuid;

use crate::controller::html::{get_user, okay, render_generic_error, render_template};
use crate::err::{AppResult, ensure_verified};
use crate::model::quotations::{CreateQuotation, QuotationRepository};
use crate::util::extract_session::Session;
use crate::AppState;

pub fn create_router() -> (Router<AppState>, Router<AppState>) {
    let secure_routes = Router::new()
        .route("/quotations/new", post(create_quotation_submit));

    let normal_routes = Router::new()
        .route("/quotations/new", get(new_quotation_form))
        .route("/quotations/{id}", get(view_quotation));

    (secure_routes, normal_routes)
}

#[derive(Template)]
#[template(path = "quotations/new.html")]
struct NewQuotationTemplate {
    user: Option<crate::model::users::User>,
    language_id: Uuid,
    error: Option<crate::err::AppError>,
}

async fn new_quotation_form(
    s: Session,
    Path(language_id): Path<Uuid>,
) -> (StatusCode, Response) {
    render_template(NewQuotationTemplate {
        user: s.user(),
        language_id,
        error: None,
    })
}

async fn create_quotation_submit(
    s: Session,
    quotations: QuotationRepository,
    Form(req): Form<CreateQuotation>,
) -> (StatusCode, Response) {
    let user = match ensure_verified(&s) {
        Ok(u) => u,
        Err(e) => return render_generic_error(e),
    };

    match quotations.create(user, req.clone()).await {
        Ok(quotation) => {
            // Redirect to view page
            (
                StatusCode::SEE_OTHER,
                axum::response::Redirect::to(&format!("/quotations/{}", quotation.id)).into_response()
            )
        }
        Err(e) => {
            // Re-render form with error
            render_template(NewQuotationTemplate {
                user: Some(user.clone()),
                language_id: req.language_id,
                error: Some(e),
            })
        }
    }
}

#[derive(Template)]
#[template(path = "quotations/view.html")]
struct ViewQuotationTemplate {
    user: Option<crate::model::users::User>,
    quotation: crate::model::quotations::Quotation,
}

async fn view_quotation(
    s: Session,
    Path(id): Path<Uuid>,
    quotations: QuotationRepository,
) -> (StatusCode, Response) {
    let quotation = match quotations.find_by_id(id).await {
        Ok(q) => q,
        Err(e) => return render_generic_error(e),
    };

    render_template(ViewQuotationTemplate {
        user: s.user(),
        quotation,
    })
}
```

**HTML Controller Conventions:**
- Return `(Router<AppState>, Router<AppState>)` tuple: (secure_routes, normal_routes)
- Secure routes need rate limiting (POST/PUT/DELETE operations)
- Normal routes are read-only (GET operations)
- Return `(StatusCode, Response)` from handlers
- Use `#[derive(Template)]` with `#[template(path = "...")]`
- Use helper functions:
  - `render_template(template)` - renders Askama template
  - `render_generic_error(error)` - renders error page
  - `get_user!(&s)` - macro to extract user or return error
- Pass `error: Option<AppError>` to templates for validation errors
- Use `Form<T>` extractor for form submissions

Register in `src/controller/html/mod.rs`:
```rust
mod quotations;

// In create_router():
let (q_sec, q_norm) = quotations::create_router();
secure_routes = secure_routes.merge(q_sec);
normal_routes = normal_routes.merge(q_norm);
```

#### 6. Create templates

Create `templates/quotations/new.html`:

```html
{% extends "layout.html" %}
{% import "util.html" as util %}

{% block title %}New Quotation{% endblock %}

{% block content %}
<div class="container">
    <h1>Add a Quotation</h1>

    {% if let Some(error) = error %}
        {{ util::top_level_error(error) }}
    {% endif %}

    <form method="post" action="/quotations/new">
        <input type="hidden" name="language_id" value="{{ language_id }}">

        <div class="form-group">
            <label for="text">Quotation Text</label>
            <textarea
                id="text"
                name="text"
                required
                rows="4"
            ></textarea>
            {% if let Some(error) = error %}
                {{ util::field_error("text", error) }}
            {% endif %}
        </div>

        <div class="form-group">
            <label for="author_name">Author (optional)</label>
            <input
                type="text"
                id="author_name"
                name="author_name"
            >
            {% if let Some(error) = error %}
                {{ util::field_error("author_name", error) }}
            {% endif %}
        </div>

        <div class="form-group">
            <label for="source">Source (optional)</label>
            <input
                type="text"
                id="source"
                name="source"
            >
            {% if let Some(error) = error %}
                {{ util::field_error("source", error) }}
            {% endif %}
        </div>

        <button type="submit">Create Quotation</button>
    </form>
</div>
{% endblock %}
```

Create `templates/quotations/view.html`:

```html
{% extends "layout.html" %}

{% block title %}Quotation{% endblock %}

{% block content %}
<div class="container">
    <blockquote>
        {{ quotation.text }}
    </blockquote>

    {% if let Some(author) = quotation.author_name %}
        <p class="author">— {{ author }}</p>
    {% endif %}

    {% if let Some(source) = quotation.source %}
        <p class="source">Source: {{ source }}</p>
    {% endif %}
</div>
{% endblock %}
```

**Template Conventions:**
- Extend `layout.html` for consistent page structure
- Import `util.html` for macros: `{% import "util.html" as util %}`
- Use `{% block title %}` and `{% block content %}`
- Display validation errors with `{{ util::field_error("field_name", error) }}`
- Display top-level errors with `{{ util::top_level_error(error) }}`
- Use standard form structure with `form-group` divs
- Access user with `{% if let Some(user) = user %}`

### Adding a Simple HTML Page (No Database)

If you just need a static or simple dynamic page:

1. Create controller function in appropriate `src/controller/html/*.rs`
2. Create template in `templates/`
3. Register route

Example - adding an "About" page:

```rust
// In src/controller/html/misc.rs (or create new file)

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {
    user: Option<User>,
}

async fn about(s: Session) -> (StatusCode, Response) {
    render_template(AboutTemplate {
        user: s.user(),
    })
}

// Register in create_router():
normal_routes = normal_routes.route("/about", get(about));
```

### Adding Frontend Interactivity

#### For a simple interactive component:

1. Create `frontend/src/my-component.ts`:

```typescript
export class MyComponent extends HTMLElement {
    connectedCallback() {
        this.innerHTML = `<button>Click me</button>`;
        this.querySelector('button')?.addEventListener('click', () => {
            alert('Clicked!');
        });
    }
}

customElements.define('my-component', MyComponent);
```

2. Import in `frontend/src/main.ts`:
```typescript
import './my-component';
```

3. Use in templates:
```html
<my-component></my-component>
```

4. Build with `just build-frontend`

#### For Alpine.js reactive behavior:

Use `x-data`, `x-on`, `x-show`, etc. directly in templates:

```html
<div x-data="{ open: false }">
    <button @click="open = !open">Toggle</button>
    <div x-show="open">Content</div>
</div>
```

### Adding Validation

Use the `validator` crate in request structs:

```rust
use validator::Validate;

#[derive(Validate, Serialize, Deserialize)]
pub struct CreateThing {
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[validate(email)]
    pub email: String,

    #[validate(range(min = 0, max = 100))]
    pub count: i32,

    #[validate(url)]
    pub website: Option<String>,
}
```

Validation automatically runs in the repository when you call `req.validate()?`. Errors are returned as `AppError` with `validation_errors` field that templates can display.

### Adding Tests

Add tests at the bottom of model files or in separate test modules:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_app;

    #[tokio::test]
    async fn test_create_quotation() {
        let app = test_app().await;
        let user = app.make_authed_user().await;

        let repo = QuotationRepository::new(app.state.clone());

        let req = CreateQuotation {
            text: "Test quotation".to_string(),
            author_name: Some("Author".to_string()),
            source: None,
            language_id: /* some language UUID */,
        };

        let quotation = repo.create(&user, req).await.unwrap();
        assert_eq!(quotation.text, "Test quotation");
    }
}
```

Run with `just test`.

## Important Patterns & Conventions

### Authentication & Authorization

**Get current user:**
```rust
async fn my_handler(s: Session) -> (StatusCode, Response) {
    let user = s.user(); // Returns Option<User>
    if user.is_none() {
        // Not logged in
    }
}
```

**Require verified user:**
```rust
async fn my_handler(s: Session) -> AppResult<Json<Thing>> {
    let user = ensure_verified(&s)?; // Returns error if not logged in or not verified
    // user is now &User
}
```

**Check permissions:**
```rust
let perms = LanguagePermissionRepository::new(state);
let can_edit = perms.can_edit_language(&user, language_id).await?;
if !can_edit {
    return Err(forbidden("You don't have permission"));
}
```

### Error Handling

**Return errors:**
```rust
use crate::err::{not_found, bad_request, forbidden, unauthorized_no_session, internal_error};

// Not found
return Err(not_found("Resource not found"));

// Bad request (validation failed)
return Err(bad_request("Invalid input"));

// Forbidden (no permission)
return Err(forbidden("You don't have access"));

// Unauthorized
return Err(unauthorized_no_session());
```

**Validation errors:**
Automatically generated by `req.validate()?` - they're displayed in templates via `util::field_error()`.

### SQL Queries

**Always use sqlx macros** for compile-time checking:

```rust
// Query returning rows
let things = sqlx::query_as!(
    Thing,
    "select * from things where user_id = $1",
    user_id
)
.fetch_all(&self.state.pool)
.await?;

// Query not returning rows
sqlx::query!(
    "delete from things where id = $1",
    id
)
.execute(&self.state.pool)
.await?;

// Optional result
let thing = sqlx::query_as!(
    Thing,
    "select * from things where id = $1",
    id
)
.fetch_optional(&self.state.pool)
.await?
.ok_or_else(|| not_found("Thing not found"))?;
```

**Consult migrations** (`@migrations`) to understand table structure. SQLx will error at compile time if your query doesn't match the database schema.

### Transactions

For multi-step operations:

```rust
let mut tx = self.state.pool.begin().await?;

sqlx::query!(
    "insert into things (name) values ($1)",
    name
)
.execute(&mut *tx)
.await?;

sqlx::query!(
    "update other_things set count = count + 1"
)
.execute(&mut *tx)
.await?;

tx.commit().await?;
```

### Pagination

Use `PaginatedRequest` and `PaginatedResponse`:

```rust
use crate::pagination::{PaginatedRequest, PaginatedResponse};

async fn list_things(
    Query(pagination): Query<PaginatedRequest>,
    repo: ThingRepository,
) -> AppResult<Json<PaginatedResponse<Thing>>> {
    let limit = pagination.limit();
    let offset = pagination.offset();

    let things = repo.list(limit, offset).await?;
    let total = repo.count().await?;

    Ok(Json(PaginatedResponse::new(things, total, offset, limit)))
}
```

## Tips & Gotchas

### Database Connection Errors

From CLAUDE.md: "sometimes the db explodes and i get a bunch of errors in my ide and i think you get told about them but you can ignore the sqlx failing to connect to database queries its fine"

If you see SQLx connection errors, the database container might need restarting:
```bash
just db
```

### Testing HTML Changes

From CLAUDE.md: "when you need to test changes to the html controllers, don't bother running `just test`, do `just build`"

`just build` is much faster for testing template/HTML controller changes.

### Route Parameters

**Use `{id}` NOT `:id`** - Axum uses curly braces:
```rust
// Correct
.route("/things/{id}", get(get_thing))

// Wrong
.route("/things/:id", get(get_thing))
```

### Secure vs Normal Routes

From CLAUDE.md: "secure routes" means "has user security implications", not "needs authentication"

Secure routes get rate limiting applied. Generally:
- **Secure:** POST, PUT, PATCH, DELETE (mutations)
- **Normal:** GET (reads)

### Repository Extraction

Repositories are automatically extracted from `AppState` by Axum. Just add them as parameters:

```rust
async fn my_handler(
    s: Session,
    things: ThingRepository,
    other_things: OtherThingRepository,
) -> AppResult<Json<Thing>> {
    // Repositories are ready to use
    things.find_by_id(id).await?
}
```

This works via the `repo_from_parts!` macro defined in each model.

### Frontend Build

TypeScript/CSS in `frontend/src/` is compiled to `frontend/dist/` and served from `/static/` path.

### Migrations Run Automatically

Migrations in `migrations/` run automatically on app startup. No manual migration commands needed.

## Common Template Macros

From `templates/util.html`:

```html
{# Display validation error for a field #}
{{ util::field_error("field_name", error) }}

{# Display top-level error message #}
{{ util::top_level_error(error) }}

{# User display name or username #}
{{ util::name(user) }}

{# User profile picture #}
{{ util::pfp(user, 50) }}

{# User badge with picture and name #}
{{ util::badge(user, 30) }}

{# Relative timestamp (e.g., "2 hours ago") #}
{{ util::relative(timestamp) }}

{# Activity feed item #}
{{ util::make_activity(activity) }}

{# Language description summary (first sentence) #}
{{ util::language_summary(language) }}

{# Translatable card display #}
{{ util::translatable_card(translatable_with_liked) }}
```

## Debugging Strategies

### Check What SQL is Generated

SQLx macros print SQL at compile time. Check compiler output to see the exact queries.

### Use Print Debugging

Standard Rust debugging with `println!` or `dbg!` works:

```rust
println!("User: {:?}", user);
dbg!(&quotation);
```

### Test Database State

Connect to the test database during `just test`:

```bash
# In another terminal while tests are running
docker exec -it axismundi-postgres-test psql -U postgres -d axismundi
```

### Check Frontend Console

Open browser DevTools console to see JavaScript errors and `console.log` output.

### Read Error Messages Carefully

AppError messages are designed to be helpful. The error contains:
- Human-readable message
- HTTP status code
- Validation errors (if applicable)

## When in Doubt

1. **Look at similar code** - Find an existing feature that's similar to what you're building
2. **Check migrations** - Use `@migrations` to see exact table structures
3. **Run tests** - `just test` catches many issues
4. **Build incrementally** - Test each piece before moving on
5. **Ask the user** - If requirements are unclear, ask rather than guessing

## Architecture Principles

This codebase follows these patterns:

1. **Repository pattern** - All database access goes through repositories
2. **Separation of concerns** - API vs HTML controllers are separate
3. **Validation at the edge** - Validate input as early as possible
4. **Type safety** - Compile-time checked SQL queries, strong typing throughout
5. **Simplicity** - Avoid over-engineering, keep things straightforward

Following these patterns will make your code fit naturally into the existing codebase.

## Template patterns

generally:
- headings and titles are lowercase
- full buttons are Title Case, smaller buttons (e.g. in a header-with-actions) are lowercase
- 

### Lists

there are 2 kinds of lists: lists on search pages, and preview lists. preview lists are introduced by a `header-with-actions`
which has a [+ new $RESOURCE ] action and a [= view all ] action, then have 3 cards. search lists are
introduced by a search bar and have pagination after them

### Cards

resources are represented by cards. there are generally 3 patterns to cards: either it's a clickable card
(points to a full resource page), or it's a card with actions (has an edit and delete button usually), or
it's a full card (on its own resource page). clickable cards and cards with actions appear in lists, and
can be referred to as "preview cards"

#### Preview cards

preview cards have a structure of information on the left and actions on the right. the information on
the left is 3 lines: the main identifying piece of information (a user's name, a word's normal form),
a summary (the first line of a language's description, the word's first definition), and a "created by / at"
section. the right side will either have a like button, edit/delete actions, or nothing

#### Full cards

full cards are a bit more idiosyncratic and ill avoid noting too many general patterns. the language card is a good reference

### User badges

user badges can be small, medium, or large. they are either clickable or not


