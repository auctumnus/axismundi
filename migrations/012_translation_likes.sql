-- Add like_count to translation table
alter table translation add column like_count bigint not null default 0;

-- Create translation_likes table
create table translation_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    translation_id uuid not null references translation(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, translation_id)
);

create index idx_translation_likes_translation_id on translation_likes(translation_id);
