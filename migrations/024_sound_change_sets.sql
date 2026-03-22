create table sound_change_sets (
    id uuid primary key default uuidv7(),
    language_id uuid not null references languages(id) on delete cascade,

    name text not null,
    description text,
    changes text not null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);