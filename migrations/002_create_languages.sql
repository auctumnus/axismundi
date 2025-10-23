create table languages (
    id uuid primary key default uuidv7(),
    code text not null unique,
    name text not null,

    description text not null,

    private boolean not null default false,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,

    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

