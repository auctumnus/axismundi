-- any resource which can be mutated and should have an audit log entry
-- audit log entries are produced when a moderator acts on a resource
-- they do not normally have access to
create type auditable_resource as enum (
    'user',
    'language',
    'word',
    'translation',
    'translatable',
    'word_relation',
    'invite',
    'permission',
    'quotation',
    'definition',
    'quotation_suggestion',
    'report'
);

create type audit_action_type as enum (
    'created',
    'updated',
    'deleted',
    'updated_report', -- any kind of report update (status change, note added, etc)
    'user_ban',
    'user_unban',
    'add_tag',
    'remove_tag'
);

create table audit_logs (
    id uuid primary key default uuidv7(),
    user_id uuid references users(id) on delete set null,

    action audit_action_type not null,
    action_at timestamp with time zone not null default current_timestamp,

    resource_type auditable_resource not null,
    resource_id uuid not null,

    -- json blob which may be differently formatted for any resource type
    -- or action. don't try to parse this!
    -- actions which generate a log should try to include as much of the
    -- request as possible
    details jsonb not null
);

create index idx_audit_logs_action_at on audit_logs(action_at);
create index idx_audit_logs_resource_type_id on audit_logs(resource_type, resource_id);
create index idx_audit_logs_user_id on audit_logs(user_id);
create index idx_audit_logs_action on audit_logs(action);