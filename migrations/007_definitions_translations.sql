create table definitions (
    id uuid primary key default uuidv7(),

    word uuid not null references words(id) on delete cascade,

    definition text not null,
    context text not null default '',
    position integer not null default 0,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

CREATE INDEX definitions_definition_trgm_idx ON definitions USING GIN (definition gin_trgm_ops);

create index idx_definitions_word on definitions(word);

create table translatable (
    id uuid primary key default uuidv7(),

    slug text not null unique,
    title text not null,

    english text not null,
    source_name text not null default '',
    source_url text not null default '',
    source_content text not null default '',
    source_language text not null default '',
    description text not null default '',

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

CREATE INDEX translatable_title_trgm_idx ON translatable USING GIN (title gin_trgm_ops);
CREATE INDEX translatable_english_trgm_idx ON translatable USING GIN (english gin_trgm_ops);

create index idx_created_by_translatable on translatable(created_by);
create index idx_updated_by_translatable on translatable(updated_by);

create table translation (
    id uuid primary key default uuidv7(),

    translatable uuid not null references translatable(id) on delete cascade,
    language uuid not null references languages(id) on delete cascade,

    translated_text text not null,
    translated_title text not null default '',

    ipa text not null default '',
    gloss text not null default '',
    notes text not null default '',

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

CREATE INDEX translation_translated_text_trgm_idx ON translation USING GIN (translated_text gin_trgm_ops);

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

