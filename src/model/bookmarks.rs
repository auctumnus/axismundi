use crate::err::{AppResult, not_found};
use crate::model::{
    languages::LanguageRepository,
    users::UserRepository,
    words::WordRepository,
    word_classes::WordClassRepository,
};
use crate::util::AppState;
use async_trait::async_trait;
use uuid::Uuid;

pub enum LinkType {
    Web,
    Api,
}

#[async_trait]
pub trait ResolveBookmark: Send + Sync {
    async fn resolve_bookmark(&self, item: Uuid, link_type: LinkType) -> AppResult<String>;
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "resource_type", rename_all = "snake_case")]
pub enum ResourceType {
    User,
    Language,
    Lemma,
    WordClass,
    UserSession,
}

impl ResourceType {
    fn as_repository(&self, state: &AppState) -> Box<dyn ResolveBookmark + '_> {
        match self {
            ResourceType::User => Box::new(UserRepository::new(state.clone())),
            ResourceType::Language => Box::new(LanguageRepository::new(state.clone())),
            ResourceType::Lemma => Box::new(WordRepository::new(state.clone())),
            ResourceType::WordClass => Box::new(WordClassRepository::new(state.clone())),
            ResourceType::UserSession => panic!("UserSession bookmarks are not supported"),
        }
    }
}

pub struct Bookmark {
    pub id: Uuid,
    pub slug: String,
    pub item: Uuid,
    pub resource: ResourceType,
}

pub struct BookmarkRepository {
    state: AppState,
}

const BOOKMARK_ALPHABET: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J',
    'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

impl BookmarkRepository {
    pub fn generate_slug() -> String {
        // https://zelark.github.io/nano-id-cc/
        // ID length: 15 characters
        // Speed: 1000 IDs per /second
        // ~125 years or 3T IDs needed, in order to have a 1% probability of at least one collision.
        nanoid::nanoid!(15, BOOKMARK_ALPHABET)
    }

    pub async fn resolve_bookmark(&self, item: Uuid, resource: ResourceType, link_type: LinkType) -> AppResult<String> {
        let repository = resource.as_repository(&self.state);
        repository.resolve_bookmark(item, link_type).await
    }

    pub async fn get_by_slug(&self, slug: &str) -> AppResult<Bookmark> {
        let record = sqlx::query_as!(
            Bookmark,
            r#"SELECT id, slug, item, resource as "resource: ResourceType" FROM bookmarks WHERE slug = $1"#,
            slug
        )
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(bookmark) = record {
            Ok(bookmark)
        } else {
            Err(not_found(format!("permalink with slug '{slug}'")))
        }
    }

    pub async fn get_by_resource(&self, item: Uuid, resource: ResourceType) -> AppResult<Bookmark> {
        let record = sqlx::query_as!(
            Bookmark,
            r#"SELECT id, slug, item, resource as "resource: ResourceType" FROM bookmarks WHERE item = $1 AND resource = $2"#,
            item,
            resource as _
        )
        .fetch_optional(&self.state.pool)
        .await?;

        if let Some(bookmark) = record {
            return Ok(bookmark);
        }

        let slug = Self::generate_slug();

        let bookmark = sqlx::query_as!(
            Bookmark,
            r#"INSERT INTO bookmarks (slug, item, resource) VALUES ($1, $2, $3) RETURNING id, slug, item, resource as "resource: ResourceType""#,
            slug,
            item,
            resource as _
        )
        .fetch_one(&self.state.pool)
        .await?;

        Ok(bookmark)
    }

    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

crate::util::repo_from_parts!(BookmarkRepository);
