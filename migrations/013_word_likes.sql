-- Add like_count to words table
alter table words add column like_count bigint not null default 0;

-- Create word_likes table
create table word_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    word_id uuid not null references words(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, word_id)
);

create index idx_word_likes_word_id on word_likes(word_id);
