# API Documentation

## Pagination

pagination is cursor-based on the internal uuid. your query params should look like:

```typescript
{
    "limit": 100,
    "cursor": "67e55044-10b1-426f-9247-bb680e5fe0c8", // or null
    "direction": "forwards" // or "backwards"
}
```

## Search

search will usually take a `q` query param for text search over text fields

## Users

### Get User
```
GET /users/{username}
```

retrieves a user by username

**Response**
```json
{
  "id": 1,
  "username": "example",
  "email": "user@example.com",
  "verified": true,
  "profile_picture_url": "https://..."
}
```

### List/Search Users
```
GET /users
```

lists all users with optional search filters. supports cursor-based pagination.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on username and description
- `created_before` (optional): filter users created before this timestamp
- `created_after` (optional): filter users created after this timestamp
- `limit` (optional): number of results per page (default 100)
- `cursor` (optional): pagination cursor
- `direction` (optional): pagination direction (forwards/backwards)

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
      "created_at": "2025-10-20T12:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z"
    }
  ],
  "pages_left": 1,
  "next_cursor": "uuid-here",
  "previous_cursor": null
}
```

### Create User
```
POST /users
```

creates a new user account

**Request Body**
```json
{
  "username": "example",
  "email": "user@example.com",
  "password": "password123"
}
```

**Response**
```json
{
  "id": 1,
  "username": "example",
  "email": "user@example.com",
  "verified": false,
  "profile_picture_url": null
}
```



### Verify User
```
POST /users/{id}/verify
```

verifies a user's email address

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
PUT /users/{username}/profile-picture
```

uploads a profile picture for the authenticated user

**Authentication**: Required

**Request**: `multipart/form-data`
- `image`: image file (max 5MB)

**Response**
```json
{
  "profile_picture_url": "https://..."
}
```

### List Languages User Contributes To

```
GET /users/{username}/languages/edited
```

redirects to `/languages?edited_by={username}` - retrieves all languages where the user has editor, admin, or owner permissions

### List Languages Owned by User

```
GET /users/{username}/languages/owned
```

redirects to `/languages?owned_by={username}` - retrieves all languages where the user is the owner

## Sessions

### Login
```
POST /sessions
```

creates a new session

**Request Body**
```json
{
  "email": "user@example.com",
  "password": "password123"
}
```

**Response**
```json
{
  "token": "session_token",
  "expires_at": "2025-10-21T12:00:00Z"
}
```

sets `session` cookie



### Get Sessions
```
GET /sessions
```

retrieves all sessions for the authenticated user

**Authentication**: Required

**Response**
```json
[
  {
    "id": "session_id",
    "user_id": 1,
    "expires_at": "2025-10-21T12:00:00Z",
    "created_at": "2025-10-20T12:00:00Z"
  }
]
```

## Languages

### Create Language

```
POST /languages
```

creates a new language. authenticated users only. the creating user becomes the owner.

**Authentication**: Required

**Validation**: cannot use `search` as the language code

**Request Body**
```json
{
  "code": "tlh",
  "name": "Klingon",
  "description": "warrior language from star trek"
}
```

### Get Language by Code

```
GET /languages/{code}
```

retrieves details about a specific language

**Response**
```json
{
  "code": "tlh",
  "name": "Klingon",
  "description": "warrior language from star trek",
  "owner_username": "marc_okrand",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-10-21T12:00:00Z"
}
```

### List/Search Languages

```
GET /languages
```

lists all languages with optional search filters. supports cursor-based pagination.

**Query Parameters**
- `owned_by` (optional): filter to languages owned by this username
- `edited_by` (optional): filter to languages where this user has editor/admin/owner permissions (comma-separated usernames)
- `q` (optional): search text for fuzzy matching on language name and description
- `created_before` (optional): filter languages created before this timestamp
- `created_after` (optional): filter languages created after this timestamp
- `limit` (optional): number of results per page (default 100)
- `cursor` (optional): pagination cursor
- `direction` (optional): pagination direction (forwards/backwards)

**Response**
```json
{
  "items": [
    {
      "code": "tlh",
      "name": "Klingon",
      "description": "warrior language from star trek",
      "owner_username": "marc_okrand",
      "created_at": "2025-01-01T00:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z"
    }
  ],
  "pages_left": 1,
  "next_cursor": "uuid-here",
  "previous_cursor": null
}
```

### Delete Language

```
DELETE /languages/{code}
```

deletes a language. only the owner can delete a language.

**Authentication**: Required (must be owner)

**Response**: `204 No Content`

### Edit Language

```
PUT /languages/{code}
```

updates language metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "name": "Updated Name",
  "description": "updated description"
}
```

### Get Owner of Language

```
GET /languages/{code}/owner
```

redirects to `/users/{username}` for the language owner

### Get Editors of Language

```
GET /languages/{code}/editors
```

lists all users with editor, admin, or owner permissions for this language. supports cursor-based pagination.

**Response**
```json
{
  "items": [
    {
      "username": "editor_user",
      "permission_level": "editor"
    }
  ],
  "pages_left": 0,
  "next_cursor": null,
  "previous_cursor": null
}
```

### Get Language Permissions

```
GET /languages/{code}/permissions
```

lists all permission assignments for a language. requires owner or admin permissions.

**Authentication**: Required (must be owner or admin)

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
GET /languages/{code}/permissions/{username}
```

retrieves the permission level for a specific user on this language. requires editor permissions or higher.

**Authentication**: Required (must be owner, admin, or editor)

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
PUT /languages/{code}/permissions/{username}
```

modifies the permission level of a user who already has permissions.

**Authentication**: Required

**Permission Rules** (requester on left, target user on top):
|          | owner | admin | editor |
|----------|-------|-------|--------|
| owner    | n     | y     | y      |
| admin    | n     | n     | y      |
| editor   | n     | n     | n      |

**Request Body**
```json
{
  "permission_level": "admin"
}
```

### Delete User Permissions

```
DELETE /languages/{code}/permissions/{username}
```

removes a user's permissions for a language.

**Authentication**: Required

**Permission Rules** (requester on left, target user on top):
|          | owner | admin | editor |
|----------|-------|-------|--------|
| owner    | n     | y     | y      |
| admin    | n     | /     | y      |
| editor   | n     | n     | /      |

`/` = can only remove your own permissions. you can always remove your own permissions as long as you are not the owner.

### Invite User to Language

```
POST /languages/{code}/invites/{username}
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

### Delete Language Invite

```
DELETE /languages/{code}/invites/{username}
```

deletes/rejects an invitation. owners can delete any invite, admins can delete invites except those from the owner, anyone can delete/reject their own invites.

**Authentication**: Required

**Response**: `204 No Content`

### Accept Language Invite

```
POST /languages/{code}/accept-invite
```

accepts an invitation to join a language. only the invited user can accept.

**Authentication**: Required (must be the invited user)

**Response**: grants the appropriate permissions and deletes the invite

### Create Word Class

```
POST /languages/{code}/word-classes
```

creates a new word class (part of speech) for the language. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Validation**: cannot use `search` as the abbreviation

**Request Body**
```json
{
  "abbreviation": "n",
  "name": "noun",
  "description": "person, place, thing, or idea"
}
```

### List/Search Word Classes

```
GET /languages/{code}/word-classes
```

lists all word classes for a language with optional search filters. supports cursor-based pagination.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on word class name and description
- `limit` (optional): number of results per page (default 100)
- `cursor` (optional): pagination cursor
- `direction` (optional): pagination direction (forwards/backwards)

**Response**
```json
{
  "items": [
    {
      "abbreviation": "n",
      "name": "noun",
      "description": "person, place, thing, or idea",
      "created_at": "2025-10-01T00:00:00Z"
    }
  ],
  "pages_left": 0,
  "next_cursor": null,
  "previous_cursor": null
}
```

### View Word Class

```
GET /languages/{code}/word-classes/{abbreviation}
```

retrieves details about a specific word class

**Response**
```json
{
  "abbreviation": "n",
  "name": "noun",
  "description": "person, place, thing, or idea",
  "created_at": "2025-10-01T00:00:00Z"
}
```

### Edit Word Class
```
PUT /languages/{code}/word-classes/{abbreviation}
```

updates word class metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "name": "Updated Name",
  "description": "updated description"
}
```

### Delete Word Class

```
DELETE /languages/{code}/word-classes/{abbreviation}
```

deletes a word class. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Response**: `204 No Content`

### Create Word

```
POST /languages/{code}/words
```

creates a new word in the language. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "slug": "example-word",
  "word_class_abbreviation": "n",
  "romanization": "example",
  "definition": "an illustrative instance",
  "pronunciation": "/ɪɡˈzæmpəl/"
}
```

### List/Search Words

```
GET /languages/{code}/words
```

lists/searches words in a language with optional filters. supports cursor-based pagination.

**Query Parameters**
- `q` (optional): search text for fuzzy matching on word fields (romanization, definition, etc.)
- `word_class` (optional): filter by word class abbreviation
- `limit` (optional): number of results per page (default 100)
- `cursor` (optional): pagination cursor
- `direction` (optional): pagination direction (forwards/backwards)

**Response**
```json
{
  "items": [
    {
      "slug": "example-word",
      "word_class_abbreviation": "n",
      "romanization": "example",
      "definition": "an illustrative instance",
      "pronunciation": "/ɪɡˈzæmpəl/",
      "created_at": "2025-10-01T00:00:00Z",
      "updated_at": "2025-10-21T12:00:00Z"
    }
  ],
  "pages_left": 1,
  "next_cursor": "uuid-here",
  "previous_cursor": null
}
```

### Delete Word

```
DELETE /languages/{code}/words/{slug}
```

deletes a word. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Response**: `204 No Content`

### Edit Word

```
PUT /languages/{code}/words/{slug}
```

updates word metadata. requires editor permissions or higher.

**Authentication**: Required (must be at least editor)

**Request Body**
```json
{
  "romanization": "updated",
  "definition": "updated definition",
  "pronunciation": "/ʌpˈdeɪtɪd/"
}
```