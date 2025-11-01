create table word_classes (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,

    name text not null,
    abbreviation text not null,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

-- stores cognate trees
create table cognacies (
    id uuid primary key default uuidv7(),

    tree jsonb not null,

    schema_version integer not null default 1
);


create table words (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    word_class uuid references word_classes(id) on delete set null,

    word text not null,
    slug text not null,
    lemma integer not null default 0,
    definition text not null,
    ipa text,
    notes text,

    cognacy uuid references cognacies(id) on delete set null,

    extra jsonb,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create type word_relation_type as enum (
    'derived',
    'descendant',
    'compound',
    'calque',
    'borrowed',
    'related',
    'see_also'
);

create table word_relations (
    id uuid primary key default uuidv7(),

    antecedent uuid references words(id) on delete cascade,
    consequent uuid references words(id) on delete cascade,

    kind word_relation_type not null,    

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

