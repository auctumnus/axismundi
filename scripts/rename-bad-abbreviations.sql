-- one-off cleanup: find and rename word_categories / word_classes abbreviations
-- and word slugs that contain URL-breaking characters (/, ?, #, %, \) or
-- otherwise don't match the new ABBREVIATION_REGEX (^[A-Za-z0-9][A-Za-z0-9._-]*$).
--
-- run the SELECT queries first to preview affected rows. then run the UPDATEs.
-- after that, the rows can be edited normally through the UI.
--
-- replacement strategy: substitute any disallowed char with '-'. if that would
-- collide with an existing abbreviation in the same language, the update for
-- that row will fail and you'll need to rename it manually.

-- ---------------------------------------------------------------------------
-- preview
-- ---------------------------------------------------------------------------

-- word categories with bad abbreviations
select wc.id, l.code as language, wc.name, wc.abbreviation
from word_categories wc
join languages l on l.id = wc.language
where wc.abbreviation !~ '^[A-Za-z0-9][A-Za-z0-9._-]*$';

-- word classes with bad abbreviations
select wcls.id, l.code as language, wcls.name, wcls.abbreviation
from word_classes wcls
join languages l on l.id = wcls.language
where wcls.abbreviation !~ '^[A-Za-z0-9][A-Za-z0-9._-]*$';

-- words with bad slugs
select w.id, l.code as language, w.word, w.slug, w.lemma
from words w
join languages l on l.id = w.language
where w.slug ~ '[/?#%\\]' or w.slug ~ '[[:cntrl:]]';

-- ---------------------------------------------------------------------------
-- fix
-- ---------------------------------------------------------------------------

begin;

-- word categories: replace any disallowed char with '-'
update word_categories
set abbreviation = regexp_replace(abbreviation, '[^A-Za-z0-9._-]', '-', 'g')
where abbreviation !~ '^[A-Za-z0-9][A-Za-z0-9._-]*$';

-- if anything still leads with a non-alphanumeric (e.g. used to be ".foo"),
-- strip leading dashes/periods/underscores
update word_categories
set abbreviation = regexp_replace(abbreviation, '^[._-]+', '', 'g')
where abbreviation !~ '^[A-Za-z0-9]';

-- word classes: same treatment
update word_classes
set abbreviation = regexp_replace(abbreviation, '[^A-Za-z0-9._-]', '-', 'g')
where abbreviation !~ '^[A-Za-z0-9][A-Za-z0-9._-]*$';

update word_classes
set abbreviation = regexp_replace(abbreviation, '^[._-]+', '', 'g')
where abbreviation !~ '^[A-Za-z0-9]';

-- word slugs: replace URL-breaking chars and control chars with '-'. note
-- this can produce two words in the same language with the same slug; lemma
-- is the disambiguator so technically allowed, but if a slug pair like
-- 'foo/bar' + 'foo-bar' both collapse to 'foo-bar' the lemma numbering may
-- need a manual bump. preview the bad-slug select above before committing.
update words
set slug = regexp_replace(
        regexp_replace(slug, '[/?#%\\]', '-', 'g'),
        '[[:cntrl:]]', '-', 'g'
    )
where slug ~ '[/?#%\\]' or slug ~ '[[:cntrl:]]';

-- inspect, then either commit or rollback
-- commit;
-- rollback;
