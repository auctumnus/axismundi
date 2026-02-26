ALTER TABLE language_family_members ADD COLUMN title TEXT;

-- migrate existing groupings: copy notes → title
UPDATE language_family_members
SET title = COALESCE(NULLIF(notes, ''), '(unnamed grouping)')
WHERE language_id IS NULL;

-- enforce at DB level
ALTER TABLE language_family_members
    ADD CONSTRAINT grouping_requires_title
    CHECK (language_id IS NOT NULL OR (title IS NOT NULL AND title <> ''));
