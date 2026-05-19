-- activity log changes: coalesce repeat creates, record user joins.
-- new 'user_joined' activity_type for the join feed entry.
-- 'count' tracks how many events a coalesced row represents.
-- existing update_* rows are left in place; we just stop writing new ones.

alter type activity_type add value if not exists 'user_joined';

alter table user_activities add column count integer not null default 1;

create index idx_user_activities_coalesce
    on user_activities (user_id, activity, related_entity_id, timestamp desc);
