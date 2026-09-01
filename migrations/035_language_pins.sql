create table user_language_pins (
    user_id uuid not null references users(id) on delete cascade,
    language_id uuid not null references languages(id) on delete cascade,
    created_at timestamp with time zone not null default current_timestamp,

    primary key (user_id, language_id)
);

create index user_language_pins_user_created_at_idx
    on user_language_pins(user_id, created_at desc);
