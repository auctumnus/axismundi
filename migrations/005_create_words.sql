create table word_classes (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,

    name text not null unique,
    abbreviation text not null unique,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);

create table words (
    id uuid primary key default uuidv7(),

    language uuid not null references languages(id) on delete cascade,
    word_class uuid references word_classes(id) on delete set null,

    word text not null,
    slug text not null,
    definition text not null,
    ipa text,
    notes text,

    extra jsonb,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    created_by uuid not null references users(id) on delete set null,
    updated_by uuid not null references users(id) on delete set null
);