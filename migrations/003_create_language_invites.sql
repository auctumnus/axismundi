create type permission_level as enum ('viewer', 'editor', 'admin', 'owner');

create table language_invites (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    sender uuid not null references users(id) on delete cascade,
    recipient uuid not null references users(id) on delete cascade,

    permissions permission_level not null,

    sent_at timestamp with time zone not null default current_timestamp,
    accepted_at timestamp with time zone
);