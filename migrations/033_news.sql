-- mod/admin "news" announcements. published_at is null => draft;
-- otherwise the article was published at that timestamp.

alter type activity_type add value 'publish_news';

create table news (
    id uuid primary key default uuidv7(),

    slug text not null unique,
    title text not null,
    content text not null,

    published_at timestamptz null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index news_title_trgm_idx on news using gin (title gin_trgm_ops);
create index news_content_trgm_idx on news using gin (content gin_trgm_ops);

create index idx_news_created_by on news(created_by);
create index idx_news_updated_by on news(updated_by);
create index idx_news_published_at on news(published_at);
create index idx_news_drafts on news(created_by) where published_at is null;

-- mirrors migrations/015_cascade_delete_activities.sql
create or replace function delete_news_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'news')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'news');
    return OLD;
end;
$function$;

create trigger trigger_delete_news_activities
    before delete on news
    for each row
    execute function delete_news_activities();
