create type reportable_resource as enum (
    'user',
    'language',
    'language_family_res',
    'word',
    'translation',
    'translatable',
    'word_relation',
    -- might be useful for 'this thing is broken' or
    -- 'this is a spam invite'
    'invite',
    'permission'
);

create type resolution_status_type as enum (
    'pending',
    'in_progress',
    'dismissed',
    'action_taken'
);

create type report_priority as enum (
    'low',
    'medium',
    'high',
    'urgent'
);

create table reports (
    id uuid primary key default uuidv7(),
    reporter uuid references users(id) on delete set null,
    resource_type reportable_resource not null,
    resource_id uuid not null,
    reason text not null,
    reported_at timestamp with time zone not null default current_timestamp,
    
    priority report_priority not null default 'medium',

    resolved_by uuid references users(id) on delete set null,
    resolution_status resolution_status_type not null default 'pending',
    resolved_at timestamp with time zone,
    -- whether the resolution status is visible to the reporter
    -- resolution status is always available to mods/admins
    resolution_status_hidden boolean not null default false,
    
    -- optional note about the resolution; e.g. 'user warned'
    resolution_note text,
    -- whether the resolution note is visible to the reporter
    -- resolution note is always available to mods/admins
    -- cannot have a visible note if the status is hidden
    resolution_note_hidden boolean not null default false,

    user_updated_at timestamp with time zone,
    mods_updated_at timestamp with time zone,
    mods_updated_by uuid references users(id) on delete set null,

    -- todo: should we prevent duplicate reports by the same reporter
    -- on the same resource?
    -- while it would be useful to prevent spam, it would also
    -- make it so you couldn't report the same resource multiple times
    -- unique (reporter, resource_type, resource_id)

    constraint note_visible_requires_status_visible check (
        not (resolution_note_hidden = false and resolution_status_hidden = true)
    )
);

create index reports_reporter_idx on reports(reporter);
create index reports_resource_idx on reports(resource_type);
create index reports_resource_id_idx on reports(resource_id);
create index reports_resolved_by_idx on reports(resolved_by);
create index reports_resolved_at_idx on reports(resolved_at) where resolved_at is not null;
create index reports_resolution_status_idx on reports(resolution_status);
create index reports_priority_idx on reports(priority);