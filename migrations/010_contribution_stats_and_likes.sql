create table contribution_stats (
    language_id uuid not null references languages(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    word_count bigint not null default 0,
    translation_count bigint not null default 0,

    primary key (language_id, user_id)
);

create index idx_contribution_stats_language_id_user_id on contribution_stats(language_id, user_id);

create table language_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    language_id uuid not null references languages(id) on delete cascade,
    
    created_at timestamp with time zone not null default current_timestamp,
    
    unique(user_id, language_id)
);

create index idx_language_likes_language_id on language_likes(language_id);