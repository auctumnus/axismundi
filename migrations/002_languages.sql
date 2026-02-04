-- Languages and permission system

create type permission_level as enum ('viewer', 'editor', 'admin', 'owner');

create table languages (
    id uuid primary key default uuidv7(),
    code text not null unique,
    name text not null,

    description text not null,

    private boolean not null default false,

    like_count bigint not null default 0,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,

    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create table language_invites (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    sender uuid not null references users(id) on delete cascade,
    recipient uuid not null references users(id) on delete cascade,

    permissions permission_level not null,

    sent_at timestamp with time zone not null default current_timestamp,
    accepted_at timestamp with time zone
);

create table language_permissions (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    "user" uuid not null references users(id) on delete cascade,
    permission permission_level not null,

    via uuid references language_invites(id) on delete set null,

    invited_by uuid not null references users(id) on delete set null,
    invited_at timestamp with time zone not null default current_timestamp,

    accepted_at timestamp with time zone
);

-- Contribution statistics per user per language
create table contribution_stats (
    language_id uuid not null references languages(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    word_count bigint not null default 0,
    translation_count bigint not null default 0,

    primary key (language_id, user_id)
);

create index idx_contribution_stats_language_id_user_id on contribution_stats(language_id, user_id);

-- Language likes
create table language_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    language_id uuid not null references languages(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, language_id)
);

create index idx_language_likes_language_id on language_likes(language_id);
