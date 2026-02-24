//! Database seeding script for development and testing.
//!
//! Usage:
//!   just seed           # default scale
//!   just seed 0.25      # small seed
//!   just seed 5.0       # large seed
//!   just seed-fresh     # wipe and reseed

use chrono::{DateTime, Duration, Utc};
use fake::Fake;
use fake::faker::lorem::en::{Paragraph, Sentence};
use fake::faker::name::en::Name;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// Fixed seed for reproducible data (0xAX1SMUNDI in spirit)
const RNG_SEED: u64 = 0x00A5_1500_00D1;

// Pre-computed argon2 hash of "seedpassword123" - saves time vs computing at runtime
const SEED_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2VlZHNhbHQxMjM0NTY$Kf2SqXPfABxqIvBRCcm7vVF9EqzVnHRKWQlXDO7QZMY";

// Conlang-style name parts for generating language names
const NAME_PREFIXES: &[&str] = &[
    "Tol", "Vir", "Kel", "Dra", "Myr", "Zan", "Qel", "Niv", "Syl", "Eth", "Arn", "Bel", "Cor",
    "Dur", "Fen", "Gor", "Hel", "Ith", "Jor", "Kyr", "Lor", "Mor", "Ner", "Oph", "Per", "Ral",
    "Sen", "Ter", "Uth", "Val",
];

const NAME_MIDS: &[&str] = &[
    "an", "ar", "en", "ir", "or", "ur", "al", "el", "il", "ol", "ae", "ie", "oe", "ue", "ai", "ei",
    "oi", "au", "eu", "ou",
];

const NAME_SUFFIXES: &[&str] = &[
    "ish", "ian", "ese", "ic", "ean", "an", "ine", "ene", "ite", "oid", "ar", "er", "ir", "or",
    "a", "o", "i", "u", "e", "y",
];

// Syllable parts for generating words
const ONSETS: &[&str] = &[
    "", "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w", "z",
    "br", "cr", "dr", "fr", "gr", "pr", "tr", "bl", "cl", "fl", "gl", "pl", "sl", "ch", "sh", "th",
    "sk", "sp", "st", "sw", "kn", "wr", "qu",
];

const NUCLEI: &[&str] = &[
    "a", "e", "i", "o", "u", "ai", "au", "ea", "ei", "ie", "oa", "oo", "ou",
];

const CODAS: &[&str] = &[
    "", "b", "d", "f", "g", "k", "l", "m", "n", "p", "r", "s", "t", "x", "z", "ch", "ck", "ff",
    "ll", "ng", "nk", "nt", "rd", "rk", "rm", "rn", "rt", "sh", "sk", "st", "th",
];

// Standard word classes
const WORD_CLASSES: &[(&str, &str)] = &[
    ("noun", "n"),
    ("verb", "v"),
    ("adjective", "adj"),
    ("adverb", "adv"),
    ("pronoun", "pron"),
    ("preposition", "prep"),
    ("conjunction", "conj"),
    ("interjection", "int"),
];

// Famous quotes for translatables
const QUOTES: &[(&str, &str, &str)] = &[
    (
        "The only thing we have to fear is fear itself",
        "Franklin D. Roosevelt",
        "Inaugural Address",
    ),
    (
        "To be or not to be, that is the question",
        "William Shakespeare",
        "Hamlet",
    ),
    ("I think, therefore I am", "Rene Descartes", "Meditations"),
    (
        "The unexamined life is not worth living",
        "Socrates",
        "Apology",
    ),
    ("In the beginning was the Word", "Bible", "Gospel of John"),
    (
        "All that glitters is not gold",
        "William Shakespeare",
        "The Merchant of Venice",
    ),
    ("Knowledge is power", "Francis Bacon", "Meditationes Sacrae"),
    (
        "I have a dream",
        "Martin Luther King Jr.",
        "March on Washington",
    ),
    ("The truth will set you free", "Bible", "Gospel of John"),
    ("Love conquers all", "Virgil", "Eclogues"),
    ("Time flies", "Virgil", "Georgics"),
    ("Fortune favors the bold", "Virgil", "Aeneid"),
    ("Know thyself", "Delphic maxim", "Temple of Apollo"),
    ("Nothing in excess", "Delphic maxim", "Temple of Apollo"),
    (
        "Where there is love there is life",
        "Mahatma Gandhi",
        "Writings",
    ),
    (
        "The journey of a thousand miles begins with a single step",
        "Lao Tzu",
        "Tao Te Ching",
    ),
    ("Hell is other people", "Jean-Paul Sartre", "No Exit"),
    (
        "One cannot step twice into the same river",
        "Heraclitus",
        "Fragments",
    ),
    (
        "Man is the measure of all things",
        "Protagoras",
        "Fragments",
    ),
    ("I know that I know nothing", "Socrates", "Apology"),
    (
        "Beauty is truth, truth beauty",
        "John Keats",
        "Ode on a Grecian Urn",
    ),
    ("The road not taken", "Robert Frost", "Mountain Interval"),
    (
        "Do not go gentle into that good night",
        "Dylan Thomas",
        "Collected Poems",
    ),
    (
        "The world is my oyster",
        "William Shakespeare",
        "The Merry Wives of Windsor",
    ),
    (
        "All the world's a stage",
        "William Shakespeare",
        "As You Like It",
    ),
    (
        "What's in a name? A rose by any other name would smell as sweet",
        "William Shakespeare",
        "Romeo and Juliet",
    ),
    (
        "The course of true love never did run smooth",
        "William Shakespeare",
        "A Midsummer Night's Dream",
    ),
    (
        "Brevity is the soul of wit",
        "William Shakespeare",
        "Hamlet",
    ),
    (
        "This above all: to thine own self be true",
        "William Shakespeare",
        "Hamlet",
    ),
    (
        "There are more things in heaven and earth than are dreamt of in your philosophy",
        "William Shakespeare",
        "Hamlet",
    ),
    (
        "The pen is mightier than the sword",
        "Edward Bulwer-Lytton",
        "Richelieu",
    ),
    ("Actions speak louder than words", "Proverb", "Traditional"),
    (
        "A picture is worth a thousand words",
        "Proverb",
        "Traditional",
    ),
    (
        "When in Rome, do as the Romans do",
        "Proverb",
        "Traditional",
    ),
    ("The early bird catches the worm", "Proverb", "Traditional"),
    (
        "You can't judge a book by its cover",
        "Proverb",
        "Traditional",
    ),
    ("Every cloud has a silver lining", "Proverb", "Traditional"),
    (
        "Don't count your chickens before they hatch",
        "Proverb",
        "Traditional",
    ),
    ("Two wrongs don't make a right", "Proverb", "Traditional"),
    ("Better late than never", "Proverb", "Traditional"),
    (
        "The grass is always greener on the other side",
        "Proverb",
        "Traditional",
    ),
    (
        "A journey of a thousand miles begins with a single step",
        "Proverb",
        "Traditional",
    ),
    ("Practice makes perfect", "Proverb", "Traditional"),
    (
        "Absence makes the heart grow fonder",
        "Proverb",
        "Traditional",
    ),
    ("Curiosity killed the cat", "Proverb", "Traditional"),
    (
        "Birds of a feather flock together",
        "Proverb",
        "Traditional",
    ),
    ("Look before you leap", "Proverb", "Traditional"),
    ("Honesty is the best policy", "Proverb", "Traditional"),
    ("A penny saved is a penny earned", "Proverb", "Traditional"),
    (
        "The squeaky wheel gets the grease",
        "Proverb",
        "Traditional",
    ),
];

// Report reasons
const REPORT_REASONS: &[&str] = &[
    "Inappropriate content",
    "Spam or advertising",
    "Harassment or bullying",
    "Copyright violation",
    "Incorrect information",
    "Offensive language",
    "Duplicate entry",
    "Off-topic content",
    "Broken link or reference",
    "Low quality contribution",
];

// Definition templates
const DEFINITION_TEMPLATES: &[&str] = &[
    "The act of {}",
    "A person who {}",
    "A thing that {}",
    "The quality of being {}",
    "The state of {}",
    "A type of {}",
    "Something used for {}",
    "A way of {}",
    "The process of {}",
    "Relating to {}",
];

#[derive(Debug)]
struct Scale {
    users: usize,
    languages: usize,
    families: usize,
    words_per_lang: usize,
    definitions_per_word_max: usize,
    translatables: usize,
    translations_per_translatable_max: usize,
    word_relations_per_lang: usize,
    language_invites: usize,
    family_invites: usize,
    reports: usize,
}

impl Scale {
    fn from_factor(factor: f64) -> Self {
        Self {
            users: (100.0 * factor).round() as usize,
            languages: (30.0 * factor).round() as usize,
            families: (8.0 * factor).round() as usize,
            words_per_lang: (80.0 * factor).round() as usize,
            definitions_per_word_max: 3,
            translatables: (50.0 * factor).round() as usize,
            translations_per_translatable_max: 6,
            word_relations_per_lang: (20.0 * factor).round() as usize,
            language_invites: (30.0 * factor).round() as usize,
            family_invites: (15.0 * factor).round() as usize,
            reports: (25.0 * factor).round() as usize,
        }
    }
}

fn generate_language_name(rng: &mut StdRng) -> String {
    let prefix = NAME_PREFIXES.choose(rng).unwrap();
    let mid = if rng.gen_bool(0.6) {
        *NAME_MIDS.choose(rng).unwrap()
    } else {
        ""
    };
    let suffix = NAME_SUFFIXES.choose(rng).unwrap();
    format!("{prefix}{mid}{suffix}")
}

fn generate_language_code(name: &str, rng: &mut StdRng) -> String {
    let base: String = name.chars().take(3).collect();
    let suffix: u8 = rng.gen_range(0..100);
    format!("{}{:02}", base.to_lowercase(), suffix)
}

fn generate_syllable(rng: &mut StdRng) -> String {
    let onset = ONSETS.choose(rng).unwrap();
    let nucleus = NUCLEI.choose(rng).unwrap();
    let coda = CODAS.choose(rng).unwrap();
    format!("{onset}{nucleus}{coda}")
}

fn generate_word(rng: &mut StdRng) -> String {
    let syllables = rng.gen_range(1..=4);
    (0..syllables)
        .map(|_| generate_syllable(rng))
        .collect::<String>()
}

fn generate_ipa(word: &str) -> String {
    // Simple IPA approximation - just lowercase and add slashes
    format!("/{}/", word.to_lowercase())
}

fn generate_definition(rng: &mut StdRng) -> String {
    let template = DEFINITION_TEMPLATES.choose(rng).unwrap();
    let filler: String = Sentence(3..6).fake_with_rng(rng);
    template.replace("{}", &filler.to_lowercase())
}

fn generate_username(rng: &mut StdRng) -> String {
    let word = random_word::get(random_word::Lang::En);
    let suffix: u16 = rng.gen_range(1000..9999);
    format!("{word}_{suffix}").replace('-', "")
}

async fn clear_database(pool: &PgPool) -> anyhow::Result<()> {
    println!("Clearing database...");

    // Truncate in reverse dependency order
    sqlx::query("TRUNCATE users CASCADE").execute(pool).await?;

    println!("Database cleared.");
    Ok(())
}

async fn seed_users(pool: &PgPool, rng: &mut StdRng, scale: &Scale) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding {} users...", scale.users);

    let mut user_ids = Vec::with_capacity(scale.users);
    let now = Utc::now();
    let default_pfps = [
        "default-pfps/1.webp",
        "default-pfps/2.webp",
        "default-pfps/3.webp",
    ];

    for i in 0..scale.users {
        let username = generate_username(rng);
        let email = format!("user{i}@seed.local");
        let display_name: Option<String> = if rng.gen_bool(0.3) {
            Some(Name().fake_with_rng(rng))
        } else {
            None
        };
        let description: Option<String> = if rng.gen_bool(0.3) {
            Some(Paragraph(1..3).fake_with_rng(rng))
        } else {
            None
        };
        let pronouns: Option<String> = if rng.gen_bool(0.3) {
            Some(["they/them", "she/her", "he/him", "any"][rng.gen_range(0..4)].to_string())
        } else {
            None
        };

        let created_at = now - Duration::days(rng.gen_range(1..365));
        let pfp = default_pfps.choose(rng).unwrap();

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO users (username, email, password_hash, display_name, description, pronouns, profile_picture_object_id, verified_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING id
            "#
        )
        .bind(&username)
        .bind(&email)
        .bind(SEED_PASSWORD_HASH)
        .bind(&display_name)
        .bind(&description)
        .bind(&pronouns)
        .bind(pfp)
        .bind(created_at) // verified_at
        .bind(created_at)
        .fetch_one(pool)
        .await?;

        // Create bookmark for user
        let bookmark_alphabet = [
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'g',
            'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x',
            'y', 'z',
        ];
        let slug: String = (0..15)
            .map(|_| *bookmark_alphabet.choose(rng).unwrap())
            .collect();

        sqlx::query("INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, 'user')")
            .bind(&slug)
            .bind(id)
            .execute(pool)
            .await?;

        user_ids.push(id);
    }

    println!("Created {} users.", user_ids.len());
    Ok(user_ids)
}

async fn seed_user_tags(pool: &PgPool, rng: &mut StdRng, user_ids: &[Uuid]) -> anyhow::Result<()> {
    println!("Seeding user tags...");

    let mut admin_count = 0;
    let mut mod_count = 0;

    // Make first user always admin
    if let Some(&first_user) = user_ids.first() {
        sqlx::query("INSERT INTO user_tags (user_id, tag, hidden) VALUES ($1, 'admin', false)")
            .bind(first_user)
            .execute(pool)
            .await?;
        admin_count += 1;
    }

    // Give ~2% of users admin tag, ~5% moderator tag
    for &user_id in user_ids.iter().skip(1) {
        if rng.gen_bool(0.02) {
            sqlx::query("INSERT INTO user_tags (user_id, tag, hidden) VALUES ($1, 'admin', false)")
                .bind(user_id)
                .execute(pool)
                .await?;
            admin_count += 1;
        } else if rng.gen_bool(0.05) {
            sqlx::query(
                "INSERT INTO user_tags (user_id, tag, hidden) VALUES ($1, 'moderator', false)",
            )
            .bind(user_id)
            .execute(pool)
            .await?;
            mod_count += 1;
        }
    }

    println!(
        "Created {admin_count} admin tags, {mod_count} moderator tags."
    );
    Ok(())
}

async fn seed_languages(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    user_ids: &[Uuid],
) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding {} languages...", scale.languages);

    let mut language_ids = Vec::with_capacity(scale.languages);
    let mut used_codes = HashSet::new();
    let now = Utc::now();

    for _ in 0..scale.languages {
        let name = generate_language_name(rng);
        let mut code = generate_language_code(&name, rng);

        // Ensure unique code
        while used_codes.contains(&code) {
            code = generate_language_code(&name, rng);
        }
        used_codes.insert(code.clone());

        let description: String = Paragraph(2..5).fake_with_rng(rng);
        let private = rng.gen_bool(0.2);
        let creator = user_ids.choose(rng).unwrap();
        let created_at = now - Duration::days(rng.gen_range(1..300));

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO languages (code, name, description, private, created_by, updated_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $5, $6, $6)
            RETURNING id
            "#
        )
        .bind(&code)
        .bind(&name)
        .bind(&description)
        .bind(private)
        .bind(creator)
        .bind(created_at)
        .fetch_one(pool)
        .await?;

        // Create owner permission
        sqlx::query(
            r#"
            INSERT INTO language_permissions (language, "user", permission, invited_by, invited_at, accepted_at)
            VALUES ($1, $2, 'owner', $2, $3, $3)
            "#
        )
        .bind(id)
        .bind(creator)
        .bind(created_at)
        .execute(pool)
        .await?;

        language_ids.push(id);
    }

    println!("Created {} languages.", language_ids.len());
    Ok(language_ids)
}

async fn seed_language_permissions(
    pool: &PgPool,
    rng: &mut StdRng,
    language_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!("Seeding additional language permissions...");

    let mut count = 0;
    let now = Utc::now();
    let permissions = ["viewer", "editor", "admin"];

    // Add some editors/viewers to each language
    for &lang_id in language_ids {
        let num_collaborators = rng.gen_range(0..5);
        let mut added_users = HashSet::new();

        for _ in 0..num_collaborators {
            let user = user_ids.choose(rng).unwrap();
            if added_users.contains(user) {
                continue;
            }
            added_users.insert(*user);

            let permission = permissions.choose(rng).unwrap();
            let invited_at = now - Duration::days(rng.gen_range(1..200));

            let result = sqlx::query(
                r#"
                INSERT INTO language_permissions (language, "user", permission, invited_by, invited_at, accepted_at)
                VALUES ($1, $2, $3::permission_level, $4, $5, $5)
                ON CONFLICT DO NOTHING
                "#
            )
            .bind(lang_id)
            .bind(user)
            .bind(permission)
            .bind(user_ids.choose(rng).unwrap())
            .bind(invited_at)
            .execute(pool)
            .await?;

            if result.rows_affected() > 0 {
                count += 1;
            }
        }
    }

    println!("Created {count} additional permissions.");
    Ok(())
}

async fn seed_language_invites(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    language_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!("Seeding {} language invites...", scale.language_invites);

    let permissions = ["viewer", "editor", "admin"];
    let now = Utc::now();

    for _ in 0..scale.language_invites {
        let lang = language_ids.choose(rng).unwrap();
        let sender = user_ids.choose(rng).unwrap();
        let recipient = user_ids.choose(rng).unwrap();

        if sender == recipient {
            continue;
        }

        let permission = permissions.choose(rng).unwrap();
        let sent_at = now - Duration::days(rng.gen_range(1..30));

        sqlx::query(
            r#"
            INSERT INTO language_invites (language, sender, recipient, permissions, sent_at)
            VALUES ($1, $2, $3, $4::permission_level, $5)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(lang)
        .bind(sender)
        .bind(recipient)
        .bind(permission)
        .bind(sent_at)
        .execute(pool)
        .await?;
    }

    println!("Created language invites.");
    Ok(())
}

async fn seed_language_families(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    user_ids: &[Uuid],
) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding {} language families...", scale.families);

    let mut family_ids = Vec::with_capacity(scale.families);
    let mut used_codes = HashSet::new();
    let now = Utc::now();

    // Some real-world inspired family names
    let family_names = [
        "Norian",
        "Ethelic",
        "Valdric",
        "Myrian",
        "Keltic",
        "Toralian",
        "Sindric",
        "Vesperian",
        "Draconic",
        "Aetherian",
        "Terrannic",
        "Celestian",
        "Umbral",
        "Solarian",
        "Lunarian",
    ];

    for i in 0..scale.families {
        let name = if i < family_names.len() {
            family_names[i].to_string()
        } else {
            generate_language_name(rng)
        };

        let mut code = name.chars().take(4).collect::<String>().to_lowercase();
        let suffix: u8 = rng.gen_range(0..100);
        code = format!("{code}{suffix:02}");

        while used_codes.contains(&code) {
            let suffix: u8 = rng.gen_range(0..100);
            code = format!(
                "{}{:02}",
                name.chars().take(4).collect::<String>().to_lowercase(),
                suffix
            );
        }
        used_codes.insert(code.clone());

        let description: String = Paragraph(2..4).fake_with_rng(rng);
        let creator = user_ids.choose(rng).unwrap();
        let created_at = now - Duration::days(rng.gen_range(1..300));

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO language_families (code, name, description, created_by, updated_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4, $5, $5)
            RETURNING id
            "#
        )
        .bind(&code)
        .bind(&name)
        .bind(&description)
        .bind(creator)
        .bind(created_at)
        .fetch_one(pool)
        .await?;

        // Create owner permission for family
        sqlx::query(
            r#"
            INSERT INTO language_family_permissions (family, "user", permission, invited_by, invited_at, accepted_at)
            VALUES ($1, $2, 'owner', $2, $3, $3)
            "#
        )
        .bind(id)
        .bind(creator)
        .bind(created_at)
        .execute(pool)
        .await?;

        family_ids.push(id);
    }

    println!("Created {} language families.", family_ids.len());
    Ok(family_ids)
}

async fn seed_language_family_members(
    pool: &PgPool,
    rng: &mut StdRng,
    family_ids: &[Uuid],
    language_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!("Seeding language family members...");

    let now = Utc::now();
    let mut assigned_languages = HashSet::new();
    let mut member_count = 0;

    // Assign each language to at most one family as a descendant
    let mut shuffled_languages = language_ids.to_vec();
    shuffled_languages.shuffle(rng);

    for (i, &lang_id) in shuffled_languages.iter().enumerate() {
        // Assign ~70% of languages to families
        if rng.gen_bool(0.3) {
            continue;
        }

        let family_id = family_ids[i % family_ids.len()];
        let creator = user_ids.choose(rng).unwrap();
        let created_at = now - Duration::days(rng.gen_range(1..200));

        sqlx::query(
            r#"
            INSERT INTO language_family_members (family_id, language_id, relation_type, created_by, updated_by, created_at, updated_at)
            VALUES ($1, $2, 'descendant', $3, $3, $4, $4)
            ON CONFLICT DO NOTHING
            "#
        )
        .bind(family_id)
        .bind(lang_id)
        .bind(creator)
        .bind(created_at)
        .execute(pool)
        .await?;

        assigned_languages.insert(lang_id);
        member_count += 1;
    }

    println!("Created {member_count} family members.");
    Ok(())
}

async fn seed_language_family_invites(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    family_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!(
        "Seeding {} language family invites...",
        scale.family_invites
    );

    let permissions = ["viewer", "editor", "admin"];
    let now = Utc::now();

    for _ in 0..scale.family_invites {
        let family = family_ids.choose(rng).unwrap();
        let sender = user_ids.choose(rng).unwrap();
        let recipient = user_ids.choose(rng).unwrap();

        if sender == recipient {
            continue;
        }

        let permission = permissions.choose(rng).unwrap();
        let sent_at = now - Duration::days(rng.gen_range(1..30));

        sqlx::query(
            r#"
            INSERT INTO language_family_invites (family, sender, recipient, permissions, sent_at)
            VALUES ($1, $2, $3, $4::permission_level, $5)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(family)
        .bind(sender)
        .bind(recipient)
        .bind(permission)
        .bind(sent_at)
        .execute(pool)
        .await?;
    }

    println!("Created family invites.");
    Ok(())
}

async fn seed_word_classes(
    pool: &PgPool,
    rng: &mut StdRng,
    language_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<HashMap<Uuid, Vec<Uuid>>> {
    println!("Seeding word classes...");

    let mut word_classes_by_lang: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let now = Utc::now();

    for &lang_id in language_ids {
        let mut class_ids = Vec::new();
        let creator = user_ids.choose(rng).unwrap();
        let created_at = now - Duration::days(rng.gen_range(1..200));

        for (name, abbrev) in WORD_CLASSES {
            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO word_classes (language, name, abbreviation, created_by, updated_by, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $4, $5, $5)
                RETURNING id
                "#
            )
            .bind(lang_id)
            .bind(name)
            .bind(abbrev)
            .bind(creator)
            .bind(created_at)
            .fetch_one(pool)
            .await?;

            class_ids.push(id);
        }

        word_classes_by_lang.insert(lang_id, class_ids);
    }

    println!(
        "Created word classes for {} languages.",
        word_classes_by_lang.len()
    );
    Ok(word_classes_by_lang)
}

async fn seed_words(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    language_ids: &[Uuid],
    word_classes_by_lang: &HashMap<Uuid, Vec<Uuid>>,
    user_ids: &[Uuid],
) -> anyhow::Result<HashMap<Uuid, Vec<Uuid>>> {
    println!("Seeding ~{} words per language...", scale.words_per_lang);

    let mut words_by_lang: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let now = Utc::now();

    for &lang_id in language_ids {
        let mut word_ids = Vec::new();
        let class_ids = word_classes_by_lang.get(&lang_id).unwrap();
        let mut used_words = HashSet::new();

        for _ in 0..scale.words_per_lang {
            let word = generate_word(rng);

            // Ensure unique word within language
            if used_words.contains(&word) {
                continue;
            }
            used_words.insert(word.clone());

            let slug = slug::slugify(&word);
            let ipa = generate_ipa(&word);
            let word_class = class_ids.choose(rng);
            let creator = user_ids.choose(rng).unwrap();
            let created_at = now - Duration::days(rng.gen_range(1..200));
            let notes: Option<String> = if rng.gen_bool(0.2) {
                Some(Sentence(3..8).fake_with_rng(rng))
            } else {
                None
            };

            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO words (language, word_class, word, slug, ipa, notes, created_by, updated_by, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, $8)
                RETURNING id
                "#
            )
            .bind(lang_id)
            .bind(word_class)
            .bind(&word)
            .bind(&slug)
            .bind(&ipa)
            .bind(&notes)
            .bind(creator)
            .bind(created_at)
            .fetch_one(pool)
            .await?;

            word_ids.push(id);
        }

        words_by_lang.insert(lang_id, word_ids);
    }

    let total_words: usize = words_by_lang.values().map(|v| v.len()).sum();
    println!("Created {total_words} words total.");
    Ok(words_by_lang)
}

async fn seed_definitions(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    words_by_lang: &HashMap<Uuid, Vec<Uuid>>,
    user_ids: &[Uuid],
) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding definitions...");

    let mut definition_ids = Vec::new();
    let now = Utc::now();

    for word_ids in words_by_lang.values() {
        for &word_id in word_ids {
            let num_defs = rng.gen_range(1..=scale.definitions_per_word_max);

            for _ in 0..num_defs {
                let definition = generate_definition(rng);
                let context: Option<String> = if rng.gen_bool(0.3) {
                    Some(Sentence(5..10).fake_with_rng(rng))
                } else {
                    None
                };
                let creator = user_ids.choose(rng).unwrap();
                let created_at = now - Duration::days(rng.gen_range(1..200));

                let id: Uuid = sqlx::query_scalar(
                    r#"
                    INSERT INTO definitions (word, definition, context, created_by, updated_by, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $4, $5, $5)
                    RETURNING id
                    "#
                )
                .bind(word_id)
                .bind(&definition)
                .bind(&context)
                .bind(creator)
                .bind(created_at)
                .fetch_one(pool)
                .await?;

                definition_ids.push(id);
            }
        }
    }

    println!("Created {} definitions.", definition_ids.len());
    Ok(definition_ids)
}

async fn seed_word_relations(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    words_by_lang: &HashMap<Uuid, Vec<Uuid>>,
    user_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!("Seeding word relations...");

    let relation_types = ["derived", "compound", "related", "see_also"];
    let cross_lang_types = ["borrowed", "calque"];
    let now = Utc::now();
    let mut count = 0;

    let all_words: Vec<(Uuid, &Vec<Uuid>)> = words_by_lang.iter().map(|(k, v)| (*k, v)).collect();

    for (lang_id, word_ids) in words_by_lang {
        if word_ids.len() < 2 {
            continue;
        }

        for _ in 0..scale.word_relations_per_lang {
            let antecedent = word_ids.choose(rng).unwrap();

            // 80% within-language, 20% cross-language
            let (consequent, kind) = if rng.gen_bool(0.8) {
                let c = word_ids.choose(rng).unwrap();
                (c, *relation_types.choose(rng).unwrap())
            } else {
                // Cross-language relation
                let other_lang = all_words.choose(rng).unwrap();
                if other_lang.0 == *lang_id || other_lang.1.is_empty() {
                    continue;
                }
                let c = other_lang.1.choose(rng).unwrap();
                (c, *cross_lang_types.choose(rng).unwrap())
            };

            if antecedent == consequent {
                continue;
            }

            let creator = user_ids.choose(rng).unwrap();
            let created_at = now - Duration::days(rng.gen_range(1..150));

            let result = sqlx::query(
                r#"
                INSERT INTO word_relations (antecedent, consequent, kind, created_by, updated_by, created_at, updated_at)
                VALUES ($1, $2, $3::word_relation_type, $4, $4, $5, $5)
                ON CONFLICT DO NOTHING
                "#
            )
            .bind(antecedent)
            .bind(consequent)
            .bind(kind)
            .bind(creator)
            .bind(created_at)
            .execute(pool)
            .await?;

            if result.rows_affected() > 0 {
                count += 1;
            }
        }
    }

    println!("Created {count} word relations.");
    Ok(())
}

async fn seed_translatables(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    user_ids: &[Uuid],
) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding {} translatables...", scale.translatables);

    let mut translatable_ids = Vec::new();
    let now = Utc::now();
    let mut used_slugs = HashSet::new();

    for i in 0..scale.translatables {
        let (english, source_name, source_content) = match QUOTES.get(i) {
            Some(q) => (q.0.to_string(), Some(q.1.to_string()), Some(q.2.to_string())),
            None => {
                let text: String = Paragraph(1..3).fake_with_rng(rng);
                (text, None, None)
            }
        };

        let title: String = english.chars().take(50).collect();
        let mut slug = slug::slugify(&title);
        let suffix: u16 = rng.gen_range(100..999);
        slug = format!("{slug}-{suffix}");

        while used_slugs.contains(&slug) {
            let suffix: u16 = rng.gen_range(100..999);
            slug = format!("{}-{}", slug::slugify(&title), suffix);
        }
        used_slugs.insert(slug.clone());

        let creator = user_ids.choose(rng).unwrap();
        let created_at = now - Duration::days(rng.gen_range(1..200));

        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO translatable (slug, title, english, source_name, source_content, created_by, updated_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $7)
            RETURNING id
            "#
        )
        .bind(&slug)
        .bind(&title)
        .bind(&english)
        .bind(&source_name)
        .bind(&source_content)
        .bind(creator)
        .bind(created_at)
        .fetch_one(pool)
        .await?;

        translatable_ids.push(id);
    }

    println!("Created {} translatables.", translatable_ids.len());
    Ok(translatable_ids)
}

async fn seed_translations(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    translatable_ids: &[Uuid],
    language_ids: &[Uuid],
    user_ids: &[Uuid],
) -> anyhow::Result<Vec<Uuid>> {
    println!("Seeding translations...");

    let mut translation_ids = Vec::new();
    let now = Utc::now();

    for &translatable_id in translatable_ids {
        let num_translations = rng.gen_range(2..=scale.translations_per_translatable_max);
        let mut used_langs = HashSet::new();

        for _ in 0..num_translations {
            let lang = language_ids.choose(rng).unwrap();
            if used_langs.contains(lang) {
                continue;
            }
            used_langs.insert(*lang);

            // Generate a fake translation (just some generated text)
            let translated_text: String = (0..rng.gen_range(3..8))
                .map(|_| generate_word(rng))
                .collect::<Vec<_>>()
                .join(" ");

            let ipa: Option<String> = if rng.gen_bool(0.5) {
                Some(generate_ipa(&translated_text))
            } else {
                None
            };

            let notes: Option<String> = if rng.gen_bool(0.3) {
                Some(Sentence(3..8).fake_with_rng(rng))
            } else {
                None
            };

            let creator = user_ids.choose(rng).unwrap();
            let created_at = now - Duration::days(rng.gen_range(1..150));

            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO translation (translatable, language, translated_text, ipa, notes, created_by, updated_by, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $7)
                RETURNING id
                "#
            )
            .bind(translatable_id)
            .bind(lang)
            .bind(&translated_text)
            .bind(&ipa)
            .bind(&notes)
            .bind(creator)
            .bind(created_at)
            .fetch_one(pool)
            .await?;

            translation_ids.push(id);
        }
    }

    println!("Created {} translations.", translation_ids.len());
    Ok(translation_ids)
}

async fn seed_likes(
    pool: &PgPool,
    rng: &mut StdRng,
    user_ids: &[Uuid],
    language_ids: &[Uuid],
    words_by_lang: &HashMap<Uuid, Vec<Uuid>>,
    translatable_ids: &[Uuid],
    translation_ids: &[Uuid],
) -> anyhow::Result<()> {
    println!("Seeding likes...");

    let now = Utc::now();
    let mut lang_likes = 0;
    let mut word_likes = 0;
    let mut translatable_likes = 0;
    let mut translation_likes = 0;

    // Language likes (~15% of users like each public language)
    for &lang_id in language_ids {
        for &user_id in user_ids {
            if rng.gen_bool(0.15) {
                let created_at = now - Duration::days(rng.gen_range(1..100));
                let result = sqlx::query(
                    "INSERT INTO language_likes (user_id, language_id, created_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(user_id)
                .bind(lang_id)
                .bind(created_at)
                .execute(pool)
                .await?;
                if result.rows_affected() > 0 {
                    lang_likes += 1;
                }
            }
        }
    }

    // Word likes (~5% chance per user per word, sampled)
    let all_words: Vec<Uuid> = words_by_lang.values().flatten().copied().collect();
    let sample_size = (all_words.len() / 3).max(50);
    let sampled_words: Vec<_> = all_words
        .choose_multiple(rng, sample_size.min(all_words.len()))
        .collect();

    for &&word_id in &sampled_words {
        for &user_id in user_ids {
            if rng.gen_bool(0.05) {
                let created_at = now - Duration::days(rng.gen_range(1..100));
                let result = sqlx::query(
                    "INSERT INTO word_likes (user_id, word_id, created_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(user_id)
                .bind(word_id)
                .bind(created_at)
                .execute(pool)
                .await?;
                if result.rows_affected() > 0 {
                    word_likes += 1;
                }
            }
        }
    }

    // Translatable likes
    for &translatable_id in translatable_ids {
        for &user_id in user_ids {
            if rng.gen_bool(0.1) {
                let created_at = now - Duration::days(rng.gen_range(1..100));
                let result = sqlx::query(
                    "INSERT INTO translatable_likes (user_id, translatable_id, created_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(user_id)
                .bind(translatable_id)
                .bind(created_at)
                .execute(pool)
                .await?;
                if result.rows_affected() > 0 {
                    translatable_likes += 1;
                }
            }
        }
    }

    // Translation likes
    let sampled_translations: Vec<_> = translation_ids
        .choose_multiple(rng, (translation_ids.len() / 2).max(20))
        .collect();
    for &&translation_id in &sampled_translations {
        for &user_id in user_ids {
            if rng.gen_bool(0.08) {
                let created_at = now - Duration::days(rng.gen_range(1..100));
                let result = sqlx::query(
                    "INSERT INTO translation_likes (user_id, translation_id, created_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(user_id)
                .bind(translation_id)
                .bind(created_at)
                .execute(pool)
                .await?;
                if result.rows_affected() > 0 {
                    translation_likes += 1;
                }
            }
        }
    }

    println!(
        "Created likes: {lang_likes} language, {word_likes} word, {translatable_likes} translatable, {translation_likes} translation"
    );

    // Update like counts
    println!("Updating like counts...");

    sqlx::query("UPDATE languages SET like_count = (SELECT COUNT(*) FROM language_likes WHERE language_id = languages.id)")
        .execute(pool).await?;
    sqlx::query(
        "UPDATE words SET like_count = (SELECT COUNT(*) FROM word_likes WHERE word_id = words.id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("UPDATE translatable SET like_count = (SELECT COUNT(*) FROM translatable_likes WHERE translatable_id = translatable.id)")
        .execute(pool).await?;
    sqlx::query("UPDATE translation SET like_count = (SELECT COUNT(*) FROM translation_likes WHERE translation_id = translation.id)")
        .execute(pool).await?;

    println!("Like counts updated.");
    Ok(())
}

async fn seed_contribution_stats(pool: &PgPool) -> anyhow::Result<()> {
    println!("Computing contribution stats...");

    // Count words per user per language
    sqlx::query(
        r#"
        INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
        SELECT
            w.language,
            w.created_by,
            COUNT(DISTINCT w.id),
            0
        FROM words w
        GROUP BY w.language, w.created_by
        ON CONFLICT (language_id, user_id) DO UPDATE
        SET word_count = EXCLUDED.word_count
        "#,
    )
    .execute(pool)
    .await?;

    // Count translations per user per language
    sqlx::query(
        r#"
        UPDATE contribution_stats cs
        SET translation_count = subq.count
        FROM (
            SELECT t.language, t.created_by, COUNT(*) as count
            FROM translation t
            GROUP BY t.language, t.created_by
        ) subq
        WHERE cs.language_id = subq.language AND cs.user_id = subq.created_by
        "#,
    )
    .execute(pool)
    .await?;

    // Insert translation-only contributors
    sqlx::query(
        r#"
        INSERT INTO contribution_stats (language_id, user_id, word_count, translation_count)
        SELECT
            t.language,
            t.created_by,
            0,
            COUNT(*)
        FROM translation t
        WHERE NOT EXISTS (
            SELECT 1 FROM contribution_stats cs
            WHERE cs.language_id = t.language AND cs.user_id = t.created_by
        )
        GROUP BY t.language, t.created_by
        ON CONFLICT DO NOTHING
        "#,
    )
    .execute(pool)
    .await?;

    println!("Contribution stats computed.");
    Ok(())
}

async fn seed_activities(pool: &PgPool, _rng: &mut StdRng) -> anyhow::Result<()> {
    println!("Seeding user activities...");

    // Create activities for languages
    let lang_activities: Vec<(Uuid, Uuid, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, created_by, created_at FROM languages")
            .fetch_all(pool)
            .await?;

    for (lang_id, user_id, created_at) in lang_activities {
        sqlx::query(
            r#"
            INSERT INTO user_activities (user_id, activity, entity_id, entity_type, timestamp)
            VALUES ($1, 'create_language', $2, 'language', $3)
            "#,
        )
        .bind(user_id)
        .bind(lang_id)
        .bind(created_at)
        .execute(pool)
        .await?;
    }

    // Sample words for activities (don't create activity for every single word)
    let word_sample_size = 200;
    let word_activities: Vec<(Uuid, Uuid, Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT id, created_by, language, created_at
        FROM words
        ORDER BY random()
        LIMIT $1
        "#,
    )
    .bind(word_sample_size)
    .fetch_all(pool)
    .await?;

    for (word_id, user_id, lang_id, created_at) in word_activities {
        sqlx::query(
            r#"
            INSERT INTO user_activities (user_id, activity, entity_id, entity_type, related_entity_id, related_entity_type, timestamp)
            VALUES ($1, 'create_word', $2, 'word', $3, 'language', $4)
            "#
        )
        .bind(user_id)
        .bind(word_id)
        .bind(lang_id)
        .bind(created_at)
        .execute(pool)
        .await?;
    }

    // Activities for translatables
    let translatable_activities: Vec<(Uuid, Uuid, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, created_by, created_at FROM translatable")
            .fetch_all(pool)
            .await?;

    for (trans_id, user_id, created_at) in translatable_activities {
        sqlx::query(
            r#"
            INSERT INTO user_activities (user_id, activity, entity_id, entity_type, timestamp)
            VALUES ($1, 'create_translatable', $2, 'translatable', $3)
            "#,
        )
        .bind(user_id)
        .bind(trans_id)
        .bind(created_at)
        .execute(pool)
        .await?;
    }

    // Sample translations for activities
    let translation_sample_size = 100;
    let translation_activities: Vec<(Uuid, Uuid, Uuid, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT id, created_by, translatable, created_at
        FROM translation
        ORDER BY random()
        LIMIT $1
        "#,
    )
    .bind(translation_sample_size)
    .fetch_all(pool)
    .await?;

    for (translation_id, user_id, translatable_id, created_at) in translation_activities {
        sqlx::query(
            r#"
            INSERT INTO user_activities (user_id, activity, entity_id, entity_type, related_entity_id, related_entity_type, timestamp)
            VALUES ($1, 'create_translation', $2, 'translation', $3, 'translatable', $4)
            "#
        )
        .bind(user_id)
        .bind(translation_id)
        .bind(translatable_id)
        .bind(created_at)
        .execute(pool)
        .await?;
    }

    println!("Created user activities.");
    Ok(())
}

async fn seed_reports(
    pool: &PgPool,
    rng: &mut StdRng,
    scale: &Scale,
    user_ids: &[Uuid],
    language_ids: &[Uuid],
    words_by_lang: &HashMap<Uuid, Vec<Uuid>>,
) -> anyhow::Result<()> {
    println!("Seeding {} reports...", scale.reports);

    let now = Utc::now();
    let resource_types = ["user", "language", "word", "translation"];
    let priorities = ["low", "medium", "high", "urgent"];
    let statuses = [
        "pending",
        "pending",
        "pending",
        "in_progress",
        "dismissed",
        "action_taken",
    ]; // weighted towards pending

    let all_words: Vec<Uuid> = words_by_lang.values().flatten().copied().collect();

    for _ in 0..scale.reports {
        let reporter = user_ids.choose(rng).unwrap();
        let resource_type = *resource_types.choose(rng).unwrap();

        let resource_id = match resource_type {
            "user" => *user_ids.choose(rng).unwrap(),
            "language" => *language_ids.choose(rng).unwrap(),
            "word" => {
                if all_words.is_empty() {
                    continue;
                }
                *all_words.choose(rng).unwrap()
            }
            _ => continue,
        };

        let reason = REPORT_REASONS.choose(rng).unwrap();
        let priority = *priorities.choose(rng).unwrap();
        let status = *statuses.choose(rng).unwrap();
        let reported_at = now - Duration::days(rng.gen_range(1..60));

        let (resolved_by, resolved_at, resolution_note): (
            Option<Uuid>,
            Option<DateTime<Utc>>,
            Option<String>,
        ) = if status != "pending" && status != "in_progress" {
            (
                Some(*user_ids.choose(rng).unwrap()),
                Some(reported_at + Duration::days(rng.gen_range(1..14))),
                if rng.gen_bool(0.5) {
                    Some("Reviewed and addressed.".to_string())
                } else {
                    None
                },
            )
        } else {
            (None, None, None)
        };

        sqlx::query(
            r#"
            INSERT INTO reports (reporter, resource_type, resource_id, reason, priority, resolution_status, reported_at, resolved_by, resolved_at, resolution_note)
            VALUES ($1, $2::reportable_resource, $3, $4, $5::report_priority, $6::resolution_status_type, $7, $8, $9, $10)
            "#
        )
        .bind(reporter)
        .bind(resource_type)
        .bind(resource_id)
        .bind(reason)
        .bind(priority)
        .bind(status)
        .bind(reported_at)
        .bind(resolved_by)
        .bind(resolved_at)
        .bind(&resolution_note)
        .execute(pool)
        .await?;
    }

    println!("Created reports.");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Axis Mundi Database Seeder ===\n");

    // Get configuration from environment
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let scale_factor: f64 = std::env::var("SEED_SCALE")
        .unwrap_or_else(|_| "1.0".to_string())
        .parse()
        .expect("SEED_SCALE must be a number");

    let should_clear = std::env::var("SEED_CLEAR").is_ok();

    println!(
        "Database: {}",
        database_url.split('@').next_back().unwrap_or(&database_url)
    );
    println!("Scale factor: {scale_factor}");
    println!("Clear first: {should_clear}\n");

    let scale = Scale::from_factor(scale_factor);

    // Connect to database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Initialize RNG with fixed seed
    let mut rng = StdRng::seed_from_u64(RNG_SEED);

    // Clear if requested
    if should_clear {
        clear_database(&pool).await?;
    }

    // Seed in dependency order
    let user_ids = seed_users(&pool, &mut rng, &scale).await?;
    seed_user_tags(&pool, &mut rng, &user_ids).await?;

    let language_ids = seed_languages(&pool, &mut rng, &scale, &user_ids).await?;
    seed_language_permissions(&pool, &mut rng, &language_ids, &user_ids).await?;
    seed_language_invites(&pool, &mut rng, &scale, &language_ids, &user_ids).await?;

    let family_ids = seed_language_families(&pool, &mut rng, &scale, &user_ids).await?;
    seed_language_family_members(&pool, &mut rng, &family_ids, &language_ids, &user_ids).await?;
    seed_language_family_invites(&pool, &mut rng, &scale, &family_ids, &user_ids).await?;

    let word_classes = seed_word_classes(&pool, &mut rng, &language_ids, &user_ids).await?;
    let words_by_lang = seed_words(
        &pool,
        &mut rng,
        &scale,
        &language_ids,
        &word_classes,
        &user_ids,
    )
    .await?;
    seed_definitions(&pool, &mut rng, &scale, &words_by_lang, &user_ids).await?;
    seed_word_relations(&pool, &mut rng, &scale, &words_by_lang, &user_ids).await?;

    let translatable_ids = seed_translatables(&pool, &mut rng, &scale, &user_ids).await?;
    let translation_ids = seed_translations(
        &pool,
        &mut rng,
        &scale,
        &translatable_ids,
        &language_ids,
        &user_ids,
    )
    .await?;

    seed_likes(
        &pool,
        &mut rng,
        &user_ids,
        &language_ids,
        &words_by_lang,
        &translatable_ids,
        &translation_ids,
    )
    .await?;
    seed_reports(
        &pool,
        &mut rng,
        &scale,
        &user_ids,
        &language_ids,
        &words_by_lang,
    )
    .await?;

    // Compute contribution stats and create activities
    seed_contribution_stats(&pool).await?;
    seed_activities(&pool, &mut rng).await?;

    println!("\n=== Seeding complete! ===");
    println!("\nAll users have password: seedpassword123");
    println!("First user (user0@seed.local) is an admin.");

    Ok(())
}
