-- Add tags column to users table
alter table users add column tags text[] not null default '{}';

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

-- Trigger to update tags on insert
create trigger trigger_update_user_tags_on_insert
after insert on user_tags
for each row
execute function update_user_tags();

-- Trigger to update tags on update
create trigger trigger_update_user_tags_on_update
after update on user_tags
for each row
execute function update_user_tags();

-- Trigger to update tags on delete
create trigger trigger_update_user_tags_on_delete
after delete on user_tags
for each row
execute function update_user_tags();

-- Initially populate tags for all existing users
update users u
set tags = (
    select coalesce(array_agg(ut.tag order by ut.created_at), '{}')
    from user_tags ut
    where ut.user_id = u.id
    and ut.hidden = false
);
