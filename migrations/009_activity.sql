create type activity_type as enum (
    'create_word',
    'update_word',
    'create_translatable',
    'update_translatable',
    'create_translation',
    'update_translation',
    'create_language',
    'update_language'
);

create table user_activities (
    id uuid primary key default uuidv7(),

    user_id uuid not null references users(id) on delete cascade,
    activity activity_type not null,
    entity_id uuid not null,
    entity_type text not null,
    related_entity_id uuid,
    related_entity_type text,

    timestamp timestamp with time zone not null default current_timestamp
);

create index idx_user_activities_user_id on user_activities(user_id);
create index idx_user_activities_related_entity on user_activities(related_entity_id);