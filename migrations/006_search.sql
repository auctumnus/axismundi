-- Full-text search indexes using trigrams

create extension if not exists pg_trgm;

-- User search indexes
create index users_username_trgm_idx on users using gin (username gin_trgm_ops);
create index users_description_trgm_idx on users using gin (description gin_trgm_ops);

-- Language search indexes
create index languages_name_trgm_idx on languages using gin (name gin_trgm_ops);
create index languages_description_trgm_idx on languages using gin (description gin_trgm_ops);

-- Word class search indexes
create index word_classes_name_trgm_idx on word_classes using gin (name gin_trgm_ops);

-- Word search indexes
create index words_word_trgm_idx on words using gin (word gin_trgm_ops);

-- Definition search indexes
create index definitions_definition_trgm_idx on definitions using gin (definition gin_trgm_ops);

-- Translatable search indexes
create index translatable_title_trgm_idx on translatable using gin (title gin_trgm_ops);
create index translatable_english_trgm_idx on translatable using gin (english gin_trgm_ops);

-- Translation search indexes
create index translation_translated_text_trgm_idx on translation using gin (translated_text gin_trgm_ops);
