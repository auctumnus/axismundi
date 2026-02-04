-- Word definitions

create table definitions (
    id uuid primary key default uuidv7(),

    word uuid not null references words(id) on delete cascade,

    definition text not null,
    context text,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index idx_definitions_word on definitions(word);
