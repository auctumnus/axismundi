# API Documentation

all endpoints are prefixed with `/api/`

## Pagination

pagination is offset-based. your query params should look like:

```typescript
{
    "limit": 100,
    "offset": 0
}
```

responses include:

```json
{
  "items": [...],
  "total": 123,
  "offset": 0,
  "limit": 100,
  "has_more": true
}
```

## Search

search will usually take a `q` query param for text search over text fields

## Users

### Get User
```
GET /api/users/{username}
```

retrieves a user by username. public endpoint.

**Response**
```json
{
  "username": "example",
  "display_name": "Example User",
  "description": "...",
  "pronouns": "they/them",
  "gender": "ff00ff",
  "profile_picture_url": "https://...",
  "bookmark": "slug-here",
  "created_at": "2025-10-20T12:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

note: sensitive fields like `id`, `email`, and `password_hash` are not exposed

### List/Search Users
```
GET /api/users
```

lists all users with optional search filters. supports offset-based pagination. public endpoint.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on username and description
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "username": "example",
      "display_name": "Example User",
      "description": "...",
      "pronouns": "they/them",
      "gender": "ff00ff",
      "profile_picture_url": "https://...",
      "bookmark": "slug-here",
      "created_at": "2025-10-20T12:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z"
    }
  ],
  "total": 123,
  "offset": 0,
  "limit": 100,
  "has_more": true
}
```

### Create User
```
POST /api/users
```

creates a new user account. rate limited.

**Request Body**
```json
{
  "username": "example",
  "email": "user@example.com",
  "password": "MyVerySecureAndUniquePassword2024!",
  "display_name": "Example User (optional)",
  "description": "This is a test user account (optional)",
  "pronouns": "they/them (optional)",
  "gender": "abc123 (optional)"
}
```

**Response**
```json
{
  "username": "example",
  "display_name": "Example User",
  "description": "...",
  "pronouns": "they/them",
  "gender": "abc123",
  "profile_picture_url": null,
  "bookmark": "slug-here",
  "created_at": "2025-10-20T12:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

**Validation**
- username: lowercase letters, numbers, underscores, hyphens only
- password: must be strong (minimum length, complexity requirements)
- email: must be valid email format
- username and email must be unique

### Update User
```
PUT /api/users/{username}
```

updates user profile information. authenticated users only. users can only update their own profile.

**Authentication**: Required

**Request Body**
```json
{
  "display_name": "New Display Name (optional)",
  "description": "New description (optional)",
  "pronouns": "new/pronouns (optional)",
  "gender": "newcolor (optional)"
}
```

**Response**
```json
{
  "username": "example",
  "display_name": "New Display Name",
  "description": "New description",
  "pronouns": "new/pronouns",
  "gender": "newcolor",
  "profile_picture_url": "https://...",
  "bookmark": "slug-here",
  "created_at": "2025-10-20T12:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

### Verify User
```
POST /api/users/{id}/verify
```

verifies a user's email address. rate limited.

**Request Body**
```json
{
  "token": "verification_token",
  "email": "user@example.com"
}
```

**Response**
```
200 OK
```

### Upload Profile Picture
```
PUT /api/users/{username}/profile-picture
```

uploads a profile picture for the authenticated user. users can only upload their own profile picture. public endpoint (but requires authentication).

**Authentication**: Required

**Request**: `multipart/form-data`
- `image`: image file (max 5MB)

**Response**
```json
{
  "profile_picture_url": "https://..."
}
```

### Start Password Reset
```
POST /api/reset-password/start
```

initiates a password reset flow. sends an email with a reset token. rate limited.

**Request Body**
```json
{
  "email": "user@example.com"
}
```

**Response**
```
200 OK
```

note: always returns 200 OK regardless of whether the email exists (security measure)

### Complete Password Reset
```
POST /api/reset-password/complete
```

completes the password reset using the token from the email. invalidates all existing sessions. rate limited.

**Request Body**
```json
{
  "uuid": "user-uuid-from-email",
  "token": "reset-token-from-email",
  "new_password": "NewSecurePassword123!"
}
```

**Response**
```
200 OK
```

## Sessions

### Login
```
POST /api/sessions
```

creates a new session. rate limited.

**Request Body**
```json
{
  "email": "user@example.com",
  "password": "MyVerySecureAndUniquePassword2024!"
}
```

**Response**
```json
{
  "token": "session_token",
  "expires_at": "2025-10-21T12:00:00Z"
}
```

also sets `session` cookie

### Get Sessions
```
GET /api/sessions
```

retrieves all sessions for the authenticated user

**Authentication**: Required

**Response**
```json
[
  {
    "id": "session_id",
    "expires_at": "2025-10-21T12:00:00Z",
    "created_at": "2025-10-20T12:00:00Z"
  }
]
```

## Bookmarks

bookmarks are permanent identifiers for resources that can change their slug/username/code. they allow stable linking even when the resource is renamed.

### Resolve Bookmark
```
GET /api/bookmarks/{slug}
```

resolves a bookmark to the current resource URL. public endpoint.

**Response**: `307 Temporary Redirect` to the actual resource

bookmarks work for:
- users
- languages
- words
- word classes

## Languages

### Create Language

```
POST /api/languages
```

creates a new language. authenticated users only. the creating user becomes the owner.

**Authentication**: Required

**Validation**: cannot use `search` as the language code

**Request Body**
```json
{
  "code": "tlh",
  "name": "Klingon",
  "description": "warrior language from star trek (optional)"
}
```

**Response**
```json
{
  "code": "tlh",
  "name": "Klingon",
  "description": "warrior language from star trek",
  "owner_username": "marc_okrand",
  "bookmark": "slug-here",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

### Get Language by Code

```
GET /api/languages/{code}
```

retrieves details about a specific language. public endpoint.

**Response**
```json
{
  "code": "tlh",
  "name": "Klingon",
  "description": "warrior language from star trek",
  "owner_username": "marc_okrand",
  "bookmark": "slug-here",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

### List/Search Languages

```
GET /api/languages
```

lists all languages with optional search filters. supports offset-based pagination. public endpoint.

**Query Parameters**
- `owned_by` (optional): filter to languages owned by this username
- `edited_by` (optional): filter to languages where this user has editor/admin/owner permissions
- `q` (optional): search text for fuzzy matching on language name and description
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "code": "tlh",
      "name": "Klingon",
      "description": "warrior language from star trek",
      "owner_username": "marc_okrand",
      "bookmark": "slug-here",
      "created_at": "2025-01-01T00:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z"
    }
  ],
  "total": 123,
  "offset": 0,
  "limit": 100,
  "has_more": true
}
```

### Delete Language

```
DELETE /api/languages/{code}
```

deletes a language. only the owner can delete a language.

**Authentication**: Required (must be owner)

**Response**: `204 No Content`

### Edit Language

```
PUT /api/languages/{code}
```

updates language metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "code": "new-code (optional)",
  "name": "Updated Name (optional)",
  "description": "updated description (optional)"
}
```

**Response**
```json
{
  "code": "new-code",
  "name": "Updated Name",
  "description": "updated description",
  "owner_username": "marc_okrand",
  "bookmark": "slug-here",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

### Get Owner of Language

```
GET /api/languages/{code}/owner
```

redirects to `/api/users/{username}` for the language owner. public endpoint.

**Response**: `302 Redirect`

### Get Editors of Language

```
GET /api/languages/{code}/editors
```

lists all users with editor, admin, or owner permissions for this language. supports offset-based pagination. public endpoint.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on username
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "username": "editor_user",
      "permission_level": "editor"
    }
  ],
  "total": 5,
  "offset": 0,
  "limit": 100,
  "has_more": false
}
```

### Get Language Permissions

```
GET /api/languages/{code}/permissions
```

lists all permission assignments for a language. requires editor or higher permissions.

**Authentication**: Required (must be at least editor)

**Response**
```json
[
  {
    "username": "editor_user",
    "permission_level": "editor",
    "granted_at": "2025-10-01T00:00:00Z"
  }
]
```

### Get Permissions for User

```
GET /api/languages/{code}/permissions/{username}
```

retrieves the permission level for a specific user on this language. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Response**
```json
{
  "username": "editor_user",
  "permission_level": "editor",
  "granted_at": "2025-10-01T00:00:00Z"
}
```

### Edit User Permissions

```
PUT /api/languages/{code}/permissions/{username}
```

modifies the permission level of a user who already has permissions.

**Authentication**: Required (must be admin or owner)

**Permission Rules**:
- owners can modify admin and editor permissions
- admins can modify editor permissions
- editors cannot modify permissions

**Request Body**
```json
{
  "permission_level": "admin"
}
```

**Response**
```json
{
  "username": "editor_user",
  "permission_level": "admin",
  "granted_at": "2025-10-01T00:00:00Z"
}
```

### Delete User Permissions

```
DELETE /api/languages/{code}/permissions/{username}
```

removes a user's permissions for a language.

**Authentication**: Required

**Permission Rules**:
- owners can remove admin and editor permissions
- admins can remove editor permissions and their own permissions
- editors can only remove their own permissions
- owners cannot remove their own permissions

**Response**: `204 No Content`

### Invite User to Language

```
POST /api/languages/{code}/invites/{username}
```

creates an invitation for a user to join the language with specified permissions. owners can invite anyone, admins can invite editors.

**Authentication**: Required (owner to invite admin/editor, admin to invite editor)

**Validation**: denies if an invite already exists for this user or if they already have permissions

**Request Body**
```json
{
  "permission_level": "editor"
}
```

**Response**
```json
{
  "recipient": "invitee_username",
  "sender": "sender_username",
  "permissions": "editor",
  "sent_at": "2025-10-01T00:00:00Z",
  "accepted_at": null
}
```

### Search Language Invites

```
GET /api/languages/{code}/invites
```

lists all invites for a language. requires editor or higher permissions.

**Authentication**: Required (must be at least editor)

**Query Parameters**
- `recipient` (optional): filter by recipient username
- `sender` (optional): filter by sender username
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "recipient": "invitee_username",
      "sender": "sender_username",
      "permissions": "editor",
      "sent_at": "2025-10-01T00:00:00Z",
      "accepted_at": null
    }
  ],
  "total": 5,
  "offset": 0,
  "limit": 100,
  "has_more": false
}
```

### View Language Invite

```
GET /api/languages/{code}/invites/{username}
```

retrieves a specific invite. only the sender, recipient, or language editors can view.

**Authentication**: Required

**Response**
```json
{
  "recipient": "invitee_username",
  "sender": "sender_username",
  "permissions": "editor",
  "sent_at": "2025-10-01T00:00:00Z",
  "accepted_at": null
}
```

### Delete Language Invite

```
DELETE /api/languages/{code}/invites/{username}
```

deletes/rejects an invitation. owners can delete any invite, admins can delete invites except those from the owner, recipients can reject their own invites.

**Authentication**: Required

**Response**: `204 No Content`

### Accept Language Invite

```
POST /api/languages/{code}/accept-invite
```

accepts an invitation to join a language. only the invited user can accept.

**Authentication**: Required (must be the invited user)

**Response**: `200 OK` (grants the appropriate permissions and deletes the invite)

### Create Word Class

```
POST /api/languages/{code}/word-classes
```

creates a new word class (part of speech) for the language. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Validation**: cannot use `search` as the abbreviation

**Request Body**
```json
{
  "abbreviation": "n",
  "name": "noun",
  "description": "person, place, thing, or idea (optional)"
}
```

**Response**
```json
{
  "abbreviation": "n",
  "name": "noun",
  "description": "person, place, thing, or idea",
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z"
}
```

### List/Search Word Classes

```
GET /api/languages/{code}/word-classes
```

lists all word classes for a language with optional search filters. supports offset-based pagination. public endpoint.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on word class name and description
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "abbreviation": "n",
      "name": "noun",
      "description": "person, place, thing, or idea",
      "bookmark": "slug-here",
      "created_at": "2025-10-01T00:00:00Z"
    }
  ],
  "total": 10,
  "offset": 0,
  "limit": 100,
  "has_more": false
}
```

### View Word Class

```
GET /api/languages/{code}/word-classes/{abbreviation}
```

retrieves details about a specific word class. public endpoint.

**Response**
```json
{
  "abbreviation": "n",
  "name": "noun",
  "description": "person, place, thing, or idea",
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z"
}
```

### Edit Word Class
```
PUT /api/languages/{code}/word-classes/{abbreviation}
```

updates word class metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "abbreviation": "new-abbr (optional)",
  "name": "Updated Name (optional)",
  "description": "updated description (optional)"
}
```

**Response**
```json
{
  "abbreviation": "new-abbr",
  "name": "Updated Name",
  "description": "updated description",
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z"
}
```

### Delete Word Class

```
DELETE /api/languages/{code}/word-classes/{abbreviation}
```

deletes a word class. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Response**: `204 No Content`

### Create Word

```
POST /api/languages/{code}/words
```

creates a new word in the language. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "word": "example",
  "word_class": "n",
  "definition": "an illustrative instance",
  "ipa": "/ɪɡˈzæmpəl/ (optional)",
  "notes": "optional notes (optional)",
  "extra": {} // optional json for custom fields
}
```

**Response**
```json
{
  "id": "uuid",
  "language": "language-uuid",
  "word_class": "word-class-uuid",
  "word": "example",
  "slug": "example",
  "lemma": 1,
  "definition": "an illustrative instance",
  "ipa": "/ɪɡˈzæmpəl/",
  "notes": "optional notes",
  "extra": {},
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z",
  "created_by": "user-uuid",
  "updated_by": "user-uuid"
}
```

note: `slug` is automatically generated from `word` using NFKC normalization. `lemma` is an auto-incrementing number per slug to handle homonyms.

### Get Word

```
GET /api/languages/{code}/words/{slug}/{lemma}
```

retrieves a specific word by slug and lemma. public endpoint.

**Response**
```json
{
  "id": "uuid",
  "language": "language-uuid",
  "word_class": "word-class-uuid",
  "word": "example",
  "slug": "example",
  "lemma": 1,
  "definition": "an illustrative instance",
  "ipa": "/ɪɡˈzæmpəl/",
  "notes": "optional notes",
  "extra": {},
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z",
  "created_by": "user-uuid",
  "updated_by": "user-uuid"
}
```

### List/Search Words

```
GET /api/languages/{code}/words
```

lists/searches words in a language with optional filters. supports offset-based pagination. public endpoint.

**Query Parameters**
- `text_query` (optional): search text for fuzzy matching on word fields
- `exact_slug` (optional): filter by exact slug match
- `word_class` (optional): filter by word class abbreviation
- `limit` (optional): number of results per page (default 100)
- `offset` (optional): pagination offset (default 0)

**Response**
```json
{
  "items": [
    {
      "id": "uuid",
      "language": "language-uuid",
      "word_class": "word-class-uuid",
      "word": "example",
      "slug": "example",
      "lemma": 1,
      "definition": "an illustrative instance",
      "ipa": "/ɪɡˈzæmpəl/",
      "notes": "optional notes",
      "extra": {},
      "bookmark": "slug-here",
      "created_at": "2025-10-01T00:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z",
      "created_by": "user-uuid",
      "updated_by": "user-uuid"
    }
  ],
  "total": 100,
  "offset": 0,
  "limit": 100,
  "has_more": true
}
```

### Edit Word

```
PUT /api/languages/{code}/words/{slug}/{lemma}
```

updates word metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "word": "updated (optional)",
  "word_class": "v (optional)",
  "definition": "updated definition (optional)",
  "ipa": "/ʌpˈdeɪtɪd/ (optional)",
  "notes": "updated notes (optional)",
  "extra": {} // optional
}
```

**Response**
```json
{
  "id": "uuid",
  "language": "language-uuid",
  "word_class": "word-class-uuid",
  "word": "updated",
  "slug": "updated",
  "lemma": 1,
  "definition": "updated definition",
  "ipa": "/ʌpˈdeɪtɪd/",
  "notes": "updated notes",
  "extra": {},
  "bookmark": "slug-here",
  "created_at": "2025-10-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z",
  "created_by": "user-uuid",
  "updated_by": "user-uuid"
}
```

### Delete Word

```
DELETE /api/languages/{code}/words/{slug}/{lemma}
```

deletes a word. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Response**: `204 No Content`