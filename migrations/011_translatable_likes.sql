-- Add like_count to translatable table
alter table translatable add column like_count bigint not null default 0;

-- Create translatable_likes table
create table translatable_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    translatable_id uuid not null references translatable(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, translatable_id)
);

create index idx_translatable_likes_translatable_id on translatable_likes(translatable_id);
