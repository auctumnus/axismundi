create table user_tags (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    tag text not null,
    hidden boolean not null default false,
    created_at timestamp with time zone not null default current_timestamp
);

create index idx_user_tags_user_id on user_tags(user_id);