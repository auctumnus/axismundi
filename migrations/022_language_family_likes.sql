-- add like_count to language_families
alter table language_families add column like_count bigint not null default 0;

-- create language_family_likes table
create table language_family_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    family_id uuid not null references language_families(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, family_id)
);

create index idx_language_family_likes_family_id on language_family_likes(family_id);
