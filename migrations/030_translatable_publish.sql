-- add a publish state to translatables so admins/mods can build a queue of
-- drafts to feature as "translatable of the day".
--
-- semantics: published_at is null => draft; otherwise the translatable was
-- published at that timestamp. all existing translatables were public from
-- creation, so we backfill them as published at their created_at.

alter table translatable add column published_at timestamptz null;

update translatable set published_at = created_at where published_at is null;

create index idx_translatable_published_at on translatable (published_at);
create index idx_translatable_drafts on translatable (created_by)
    where published_at is null;
