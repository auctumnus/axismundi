-- Compute eligibility on writes (including imports), and backfill existing
-- definitions automatically. This is the Unicode White_Space set used by trim().
ALTER TABLE definitions ADD COLUMN is_home_greeting boolean
    GENERATED ALWAYS AS (
        btrim(definition, U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000') = 'hello'
    ) STORED NOT NULL;

-- Home only needs the small set of matching word IDs, never a text search.
CREATE INDEX idx_definitions_home_greeting ON definitions (word)
    WHERE is_home_greeting;
