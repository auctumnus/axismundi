create table users (
    id serial primary key,
    username text not null unique,
    email text not null unique,
    password_hash text not null,

    display_name text,
    description text,
    pronouns text,
    gender text,

    profile_picture_object_id text,

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,
    
    verified_at timestamp with time zone
);

create index idx_users_email ON users(email);
create index idx_users_username ON users(username);