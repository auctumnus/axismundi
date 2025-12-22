-- Create triggers to cascade delete activities when entities are deleted

-- Trigger for words
create or replace function delete_word_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'word')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'word');
    return OLD;
end;
$function$;

create trigger trigger_delete_word_activities
    before delete on words
    for each row
    execute function delete_word_activities();

-- Trigger for translatables
create or replace function delete_translatable_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'translatable')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'translatable');
    return OLD;
end;
$function$;

create trigger trigger_delete_translatable_activities
    before delete on translatable
    for each row
    execute function delete_translatable_activities();

-- Trigger for translations
create or replace function delete_translation_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'translation')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'translation');
    return OLD;
end;
$function$;

create trigger trigger_delete_translation_activities
    before delete on translation
    for each row
    execute function delete_translation_activities();

-- Trigger for languages
create or replace function delete_language_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'language')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'language');
    return OLD;
end;
$function$;

create trigger trigger_delete_language_activities
    before delete on languages
    for each row
    execute function delete_language_activities();

-- Trigger for definitions
create or replace function delete_definition_activities()
returns trigger
language plpgsql
as $function$
begin
    delete from user_activities ua
    where (ua.entity_id = OLD.id and ua.entity_type = 'definition')
       or (ua.related_entity_id = OLD.id and ua.related_entity_type = 'definition');
    return OLD;
end;
$function$;

create trigger trigger_delete_definition_activities
    before delete on definitions
    for each row
    execute function delete_definition_activities();
