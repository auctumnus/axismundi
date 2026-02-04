-- Translatables, translations, quotations, and related tables

create table translatable (
    id uuid primary key default uuidv7(),

    slug text not null unique,
    title text not null,

    english text not null,
    source_name text,
    source_url text,
    source_content text,
    source_language text,

    like_count bigint not null default 0,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index idx_created_by_translatable on translatable(created_by);
create index idx_updated_by_translatable on translatable(updated_by);

create table translation (
    id uuid primary key default uuidv7(),

    translatable uuid not null references translatable(id) on delete cascade,
    language uuid not null references languages(id) on delete cascade,

    translated_text text not null,
    translator_name text,
    translator_url text,

    ipa text,
    gloss text,
    notes text,

    like_count bigint not null default 0,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index idx_translation_translatable on translation(translatable);
create index idx_translation_language on translation(language);

create table quotation (
    id uuid primary key default uuidv7(),

    translation uuid not null references translation(id) on delete cascade,
    definition uuid not null references definitions(id) on delete cascade,

    span_start integer not null,
    span_end integer not null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index idx_quotation_translation on quotation(translation);

create table quotation_suggestion (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    definition uuid not null references definitions(id) on delete cascade,

    span_content text not null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index idx_quotation_suggestion_language on quotation_suggestion(language);

-- Translatable likes
create table translatable_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    translatable_id uuid not null references translatable(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, translatable_id)
);

create index idx_translatable_likes_translatable_id on translatable_likes(translatable_id);

-- Translation likes
create table translation_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    translation_id uuid not null references translation(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, translation_id)
);

create index idx_translation_likes_translation_id on translation_likes(translation_id);
