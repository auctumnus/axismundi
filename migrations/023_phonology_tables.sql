create table phonology_tables (
    id uuid primary key default uuidv7(),
    language_id uuid not null references languages(id) on delete cascade,

    name text not null,
    description text,
    position integer not null default 0,

    body jsonb not null,
    schema_version integer not null default 1,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp
);