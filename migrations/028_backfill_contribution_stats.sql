-- backfill contribution_stats from words and translation tables.
-- the increment/decrement helpers were never wired into create/delete paths,
-- so every row here is stale. wipe and rebuild from authoritative sources.

truncate table contribution_stats;

insert into contribution_stats (language_id, user_id, word_count, translation_count)
select
    coalesce(w.language_id, tr.language_id) as language_id,
    coalesce(w.user_id, tr.user_id) as user_id,
    coalesce(w.word_count, 0) as word_count,
    coalesce(tr.translation_count, 0) as translation_count
from (
    select language as language_id, created_by as user_id, count(*) as word_count
    from words
    where created_by is not null
    group by language, created_by
) w
full outer join (
    select language as language_id, created_by as user_id, count(*) as translation_count
    from translation
    where created_by is not null
    group by language, created_by
) tr on w.language_id = tr.language_id and w.user_id = tr.user_id;
