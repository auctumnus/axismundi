-- also see docs/tasks.md

create type task_type as enum (
    'send_email',
    'resize_image',
    'cleanup'
);

create type task_state as enum (
    'ready',    -- job is ready to be processed
    'active',   -- a worker is processing the job
    'failed',   -- a "normal error" occurred during processing, and the job can be automatically retried
    'panicked', -- an unexpected error occurred (e.g. server crash), requires manual intervention
    'timeout'   -- the job timed out, requires manual intervention
);

create type maid_state as enum (
    'alive',
    'dead'
);

create table maids (
    id uuid primary key default uuidv7(),

    identity text not null unique,

    state maid_state not null default 'alive',

    started_at timestamp with time zone not null default current_timestamp,
    checked_in_at timestamp with time zone not null default current_timestamp
);

create table tasks (
    id uuid primary key default uuidv7(),

    type task_type not null,
    state task_state not null default 'ready',

    payload jsonb not null,

    scheduled_at timestamp with time zone not null default current_timestamp,
    started_at timestamp with time zone,
    taken_by text references maids(identity) on delete set null
);
