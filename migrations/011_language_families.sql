-- Language families (like Indo-European, Sino-Tibetan, etc.)

create table language_families (
    id uuid primary key default uuidv7(),
    code text not null unique,
    name text not null,
    description text not null,

    -- pre-computed tree structure to avoid runtime materialization
    -- schema: { edges: [{ parent_member_id, child_member_id, family_id, relation_kind }], schema_version: 1 }
    tree jsonb not null default '{"edges": [], "schema_version": 1}',

    like_count bigint not null default 0,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create index language_families_created_by_idx on language_families(created_by);

-- Relation type for family membership
create type language_family_relation_type as enum ('descendant', 'hybrid');

-- Links languages to families
-- A language has exactly ONE 'descendant' relation (its primary family lineage)
-- A language can have zero or more 'hybrid' relations (for creoles, pidgins, mixed languages)
create table language_family_members (
    id uuid primary key default uuidv7(),

    family_id uuid not null references language_families(id) on delete cascade,

    -- if no language_id, then this is a "grouping" node for organizational purposes
    language_id uuid references languages(id) on delete cascade,
    notes text not null default '',

    -- the parent language in this lineage (null = root/proto-language)
    parent_member_id uuid references language_family_members(id) on delete set null,

    relation_type language_family_relation_type not null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null,

    -- a language can only appear once per family with a given relation type and parent
    constraint language_family_members_unique
        unique (family_id, language_id, relation_type, parent_member_id),

    -- hybrid relations must have a parent (the other ancestor)
    constraint hybrid_requires_parent
        check (relation_type = 'descendant' or parent_member_id is not null)
);

create index language_family_members_family_id_idx on language_family_members(family_id);
create index language_family_members_language_id_idx on language_family_members(language_id);
create index language_family_members_parent_member_id_idx on language_family_members(parent_member_id);

-- Ensure a language has at most one 'descendant' relation across all families
-- (a language belongs to exactly one family tree as a descendant)
create unique index language_family_members_one_descendant_idx
    on language_family_members(language_id)
    where relation_type = 'descendant';

-- Invites for language families (mirrors language_invites)
-- Defined before permissions because permissions references invites
create table language_family_invites (
    id uuid primary key default uuidv7(),

    family uuid not null references language_families(id) on delete cascade,
    sender uuid not null references users(id) on delete cascade,
    recipient uuid not null references users(id) on delete cascade,

    permissions permission_level not null,

    sent_at timestamp with time zone not null default current_timestamp,
    accepted_at timestamp with time zone
);

create index language_family_invites_family_idx on language_family_invites(family);
create index language_family_invites_recipient_idx on language_family_invites(recipient);

-- Permissions for language families (mirrors language_permissions)
create table language_family_permissions (
    id uuid primary key default uuidv7(),

    family uuid not null references language_families(id) on delete cascade,
    "user" uuid not null references users(id) on delete cascade,
    permission permission_level not null,

    via uuid references language_family_invites(id) on delete set null,

    invited_by uuid not null references users(id) on delete set null,
    invited_at timestamp with time zone not null default current_timestamp,

    accepted_at timestamp with time zone
);

create index language_family_permissions_family_idx on language_family_permissions(family);
create index language_family_permissions_user_idx on language_family_permissions("user");

-- Language family likes
create table language_family_likes (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    family_id uuid not null references language_families(id) on delete cascade,

    created_at timestamp with time zone not null default current_timestamp,

    unique(user_id, family_id)
);

create index idx_language_family_likes_family_id on language_family_likes(family_id);

-- Add language_family resources to reportable_resource enum
alter type reportable_resource add value 'language_family';
alter type reportable_resource add value 'language_family_member';

-- Add language_family resources to auditable_resource enum
alter type auditable_resource add value 'language_family';
alter type auditable_resource add value 'language_family_member';
alter type auditable_resource add value 'language_family_invite';
alter type auditable_resource add value 'language_family_permission';
