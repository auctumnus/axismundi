-- Users and authentication-related tables

create table users (
    id uuid primary key default uuidv7(),
    username text not null unique,
    email text not null unique,
    password_hash text not null,

    display_name text,
    description text,
    pronouns text,
    gender text,

    profile_picture_object_id text,

    -- materialized tags from user_tags table (non-hidden only)
    tags text[] not null default '{}',

    created_at timestamp with time zone not null default current_timestamp,
    updated_at timestamp with time zone not null default current_timestamp,

    verified_at timestamp with time zone
);

create index idx_users_email on users(email);
create index idx_users_username on users(username);

create table password_reset_tokens (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create table email_verification_tokens (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    email text not null,
    token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create table user_sessions (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    session_token text not null unique,
    invalidated_at timestamp with time zone,
    created_at timestamp with time zone not null default current_timestamp,
    expires_at timestamp with time zone not null
);

create index idx_user_sessions_session_token on user_sessions(session_token);

-- User tags (badges, roles, etc.)
create table user_tags (
    id uuid primary key default uuidv7(),
    user_id uuid not null references users(id) on delete cascade,
    tag text not null,
    hidden boolean not null default false,
    created_at timestamp with time zone not null default current_timestamp
);

create index idx_user_tags_user_id on user_tags(user_id);

-- Function to update materialized tags for a user
create or replace function update_user_tags()
returns trigger as $$
declare
    target_user_id uuid;
begin
    -- Determine which user_id to use
    if TG_OP = 'DELETE' then
        target_user_id := OLD.user_id;
    else
        target_user_id := NEW.user_id;
    end if;

    -- Update the user's tags array with all non-hidden tags
    update users
    set tags = (
        select coalesce(array_agg(tag order by created_at), '{}')
        from user_tags
        where user_id = target_user_id
        and hidden = false
    )
    where id = target_user_id;

    -- Return appropriate value based on operation
    if TG_OP = 'DELETE' then
        return OLD;
    else
        return NEW;
    end if;
end;
$$ language plpgsql;

-- Triggers to update materialized tags
create trigger trigger_update_user_tags_on_insert
after insert on user_tags
for each row
execute function update_user_tags();

create trigger trigger_update_user_tags_on_update
after update on user_tags
for each row
execute function update_user_tags();

create trigger trigger_update_user_tags_on_delete
after delete on user_tags
for each row
execute function update_user_tags();

-- User bans
create table user_bans (
    id uuid primary key default uuidv7(),
    user_id uuid not null unique references users(id) on delete cascade,
    reason text not null,
    banned_at timestamp with time zone not null default current_timestamp,
    banned_by uuid not null references users(id) on delete set null
);

create index idx_user_bans_user_id on user_bans(user_id);
create index idx_user_bans_banned_by on user_bans(banned_by);
