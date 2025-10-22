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

