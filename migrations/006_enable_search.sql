CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- user search indexes
CREATE INDEX users_username_trgm_idx ON users USING GIN (username gin_trgm_ops);
CREATE INDEX users_description_trgm_idx ON users USING GIN (description gin_trgm_ops);

-- language search indexes
CREATE INDEX languages_name_trgm_idx ON languages USING GIN (name gin_trgm_ops);
CREATE INDEX languages_description_trgm_idx ON languages USING GIN (description gin_trgm_ops);

-- word class search indexes
CREATE INDEX word_classes_name_trgm_idx ON word_classes USING GIN (name gin_trgm_ops);

-- word search indexes
CREATE INDEX words_word_trgm_idx ON words USING GIN (word gin_trgm_ops);