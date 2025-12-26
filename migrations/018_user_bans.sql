create table user_bans (
    id uuid primary key default uuidv7(),
    user_id uuid not null unique references users(id) on delete cascade,
    reason text not null,
    banned_at timestamp with time zone not null default current_timestamp,
    banned_by uuid not null references users(id) on delete set null
);

create index idx_user_bans_user_id on user_bans(user_id);
create index idx_user_bans_banned_by on user_bans(banned_by);