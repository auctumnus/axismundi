-- "translatable of the day" — unified queue of draft translatables, some
-- scheduled for a specific date and the rest sitting in a deterministic
-- unscheduled order. each draft has exactly one queue row; once featured
-- (scheduled_date set), the row stays as history.
--
-- new drafts get a stable sort_key derived from the queue's seed. the
-- "today" auto-pick walks unscheduled rows by sort_key (not random), so
-- tomorrow's totd is predictable. admins can see the next-N merge-walk
-- via the upcoming view.
--
-- this is the first migration that introduces the queue; nothing to
-- migrate from a prior schema. existing drafts are backfilled below.

-- single-row config: seed (used to shuffle the queue) and the iana timezone
-- the "today" boundary should be evaluated in. the check constraint keeps
-- there from ever being more than one row.
create table totd_queue_config (
    id int primary key default 1,
    seed text not null,
    timezone text not null default 'UTC',
    constraint single_row check (id = 1)
);
insert into totd_queue_config (id, seed) values (1, gen_random_uuid()::text);

-- unified totd queue. one row per draft (scheduled_date null) and one row
-- per featured translatable (scheduled_date set, possibly in the past).
create table totd_queue (
    id              uuid primary key default gen_random_uuid(),
    translatable_id uuid not null unique
                    references translatable(id) on delete cascade,
    scheduled_date  date unique,
    sort_key        bigint not null,
    assigned_by     uuid references users(id) on delete set null,
    assigned_at     timestamptz,
    is_auto         boolean not null default false
);

-- scheduled lookups (peek window, archive) walk by date
create index idx_totd_queue_scheduled
    on totd_queue (scheduled_date)
    where scheduled_date is not null;

-- the unscheduled-queue walk pulls rows in sort_key order
create index idx_totd_queue_unscheduled
    on totd_queue (sort_key)
    where scheduled_date is null;

create index idx_totd_queue_assigned_by on totd_queue (assigned_by);

-- backfill: every existing draft gets an unscheduled row. sort_key is
-- derived from the translatable id concatenated with the queue seed.
-- hashtextextended is only stable within a postgres major version; a major
-- upgrade may reshuffle the queue, which we accept pre-launch.
insert into totd_queue (translatable_id, sort_key)
select
    t.id,
    hashtextextended(t.id::text || (select seed from totd_queue_config), 0)
from translatable t
where t.published_at is null;
