create table ipa_estimators (
    id uuid primary key default uuidv7(),
    language_id uuid not null references languages(id) on delete cascade unique,
    sound_change_set_id uuid not null references sound_change_sets(id) on delete cascade unique
);

create index idx_ipa_estimator on ipa_estimators(language_id);