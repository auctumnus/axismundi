create table word_categories (
    id uuid primary key default uuidv7(),
    language uuid not null references languages(id) on delete cascade,
    name text not null,
    abbreviation text not null,
    notes text not null default '',
    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null,
    unique (language, abbreviation),
    unique (language, name)
);

create index word_categories_language_idx on word_categories(language);
create index word_categories_name_trgm_idx on word_categories using gin (name gin_trgm_ops);

create table word_word_categories (
    word uuid not null references words(id) on delete cascade,
    category uuid not null references word_categories(id) on delete cascade,
    created_at timestamp with time zone not null default current_timestamp,
    primary key (word, category)
);

create index word_word_categories_category_idx on word_word_categories(category);

alter type resource_type add value 'word_category';
alter type auditable_resource add value 'word_category';
