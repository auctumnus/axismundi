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

create table password_reset_tokens (
    id serial primary key,
    user_id integer not null references users(id) on delete cascade,
    token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create table email_verification_tokens (
    id serial primary key,
    user_id integer not null references users(id) on delete cascade,
    email text not null,
    token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create table user_sessions (
    id serial primary key,
    user_id integer not null references users(id) on delete cascade,
    session_token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create index idx_user_sessions_session_token ON user_sessions(session_token);