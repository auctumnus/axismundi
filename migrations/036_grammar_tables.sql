create table grammar_tables (
    id uuid primary key default uuidv7(),
    language_id uuid not null references languages(id) on delete cascade,
    name text not null,
    description text not null default '',
    preamble text not null default '',
    body jsonb not null,
    position integer not null default 0,
    schema_version integer not null default 1,
    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid references users(id) on delete set null,
    updated_by uuid references users(id) on delete set null
);

create index grammar_tables_language_position_idx
    on grammar_tables (language_id, position);

create table grammar_table_word_classes (
    grammar_table_id uuid not null references grammar_tables(id) on delete cascade,
    word_class_id uuid not null references word_classes(id) on delete restrict,
    primary key (grammar_table_id, word_class_id)
);

create index grammar_table_word_classes_class_idx
    on grammar_table_word_classes (word_class_id);

create table grammar_table_categories (
    grammar_table_id uuid not null references grammar_tables(id) on delete cascade,
    category_id uuid not null references word_categories(id) on delete restrict,
    primary key (grammar_table_id, category_id)
);

create index grammar_table_categories_category_idx
    on grammar_table_categories (category_id);

create table grammar_render_cache (
    runner_version integer not null,
    source_kind text not null check (source_kind in ('ipa', 'spelling')),
    changes_hash text not null,
    input_word text not null,
    output_word text not null,
    created_at timestamp with time zone not null default current_timestamp,
    primary key (runner_version, source_kind, changes_hash, input_word)
);

alter type auditable_resource add value 'grammar_table';
