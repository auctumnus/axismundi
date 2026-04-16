create table sound_change_sets (
    id uuid primary key default uuidv7(),
    language_id uuid references languages(id) on delete cascade,
    member_id uuid references language_family_members(id) on delete cascade,

    name text not null,
    description text not null default '',
    changes text not null,

    constraint sound_change_sets_one_reference check (
        (language_id is not null) <> (member_id is not null)
    ),

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index sound_change_sets_member_id_idx on sound_change_sets(member_id);

create unique index sound_change_sets_one_per_member_idx
    on sound_change_sets(member_id)
    where member_id is not null;
