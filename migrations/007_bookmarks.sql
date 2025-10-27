create type resource_type as enum ('user', 'language', 'lemma', 'invite', 'permission', 'word_class', 'user_session');

create table bookmarks (
    id uuid primary key default uuidv7(),

    slug text not null,
    item uuid not null,
    resource resource_type not null,

    unique (slug),
    unique (item, resource)
);