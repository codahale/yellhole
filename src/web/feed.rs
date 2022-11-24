use std::ops::Range;

use askama::Template;
use atom_syndication::{Content, Entry, Feed, FixedDateTime, Link, Person, Text};
use axum::extract::{Path, Query, State};
use axum::http;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{Days, FixedOffset, NaiveDate, Utc};
use serde::Deserialize;
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;

use super::{AppError, AppState, Page};
use crate::services::notes::Note;

pub fn router() -> Router<AppState> {
    let immutable = Router::new().route("/note/:note_id", get(single)).layer(
        SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=31536000,immutable"),
        ),
    );

    Router::new()
        .route("/", get(index))
        .route("/atom.xml", get(atom))
        .route("/notes/:start", get(week))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=300"),
        ))
        .merge(immutable)
}

#[derive(Debug, Template)]
#[template(path = "feed.html")]
struct FeedPage {
    author: String,
    title: String,
    base_url: url::Url,
    notes: Vec<Note>,
    weeks: Vec<Range<NaiveDate>>,
}

impl FeedPage {
    fn from_state(state: &AppState, notes: Vec<Note>, weeks: Vec<Range<NaiveDate>>) -> FeedPage {
        FeedPage {
            author: state.author.clone(),
            title: state.title.clone(),
            base_url: state.base_url.clone(),
            notes,
            weeks,
        }
    }
}

mod filters {
    use chrono::{DateTime, Local, Utc};

    pub fn to_local_tz(t: &DateTime<Utc>) -> askama::Result<DateTime<Local>> {
        Ok(t.with_timezone(&Local))
    }
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(state: State<AppState>, opts: Query<IndexOpts>) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let notes = state.notes.most_recent(opts.n.unwrap_or(25)).await?;
    Ok(Page(FeedPage::from_state(&state, notes, weeks)))
}

async fn week(
    state: State<AppState>,
    start: Option<Path<NaiveDate>>,
) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let start = start.ok_or(AppError::NotFound)?.0;
    let notes = state.notes.date_range(start..(start + Days::new(7))).await?;
    Ok(Page(FeedPage::from_state(&state, notes, weeks)))
}

async fn single(
    state: State<AppState>,
    note_id: Option<Path<Uuid>>,
) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let note_id = note_id.ok_or(AppError::NotFound)?;
    let notes = vec![state.notes.by_id(&note_id).await?.ok_or(AppError::NotFound)?];
    Ok(Page(FeedPage::from_state(&state, notes, weeks)))
}

async fn atom(state: State<AppState>) -> Result<Response, AppError> {
    let entries = state
        .notes
        .most_recent(20)
        .await?
        .iter()
        .map(|n| Entry {
            id: state.base_url.join(&format!("note/{}", n.note_id)).unwrap().to_string(),
            title: Text { value: n.note_id.to_string(), ..Default::default() },
            content: Some(Content {
                content_type: Some("html".into()),
                value: Some(n.to_html()),
                ..Default::default()
            }),
            updated: n.created_at.with_timezone(&FixedOffset::east_opt(0).unwrap()),
            ..Default::default()
        })
        .collect();

    let feed = Feed {
        id: state.base_url.to_string(),
        authors: vec![Person { name: state.author.clone(), ..Default::default() }],
        base: Some(state.base_url.join("atom.xml").unwrap().to_string()),
        title: Text { value: state.title.clone(), ..Default::default() },
        entries,
        links: vec![Link {
            href: state.base_url.join("atom.xml").unwrap().to_string(),
            rel: "self".into(),
            ..Default::default()
        }],
        updated: FixedDateTime::from_utc(Utc::now().naive_utc(), FixedOffset::east_opt(0).unwrap()),
        ..Default::default()
    };

    Ok(([(http::header::CONTENT_TYPE, atom_xml())], feed.to_string()).into_response())
}

const fn atom_xml() -> http::HeaderValue {
    http::HeaderValue::from_static("application/atom+xml; charset=utf-8")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use axum::http::{self, StatusCode};
    use sqlx::SqlitePool;

    use crate::test_server::TestEnv;

    use super::*;

    #[sqlx::test(fixtures("notes"))]
    async fn main(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn atom_feed(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/atom.xml").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(http::header::CONTENT_TYPE), Some(&atom_xml()));

        let feed = Feed::read_from(Cursor::new(&resp.bytes().await?))?;
        assert_eq!(
            feed.entries[0].content().unwrap().value().unwrap(),
            "<p>It's a me, <em>Mario</em>.</p>\n"
        );

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn weekly_view(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/notes/2022-10-09").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn single_note(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/note/c1449d6c-6b5b-4ce4-a4d7-98853562fbf1").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn bad_note_id(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/note/not-a-uuid").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[sqlx::test(fixtures("notes"))]
    async fn missing_note_id(db: SqlitePool) -> Result<(), anyhow::Error> {
        let ts = TestEnv::new(db)?.into_server(router())?;

        let resp = ts.get("/note/37c615b0-bb55-424d-a813-69e14ca5c20c").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }
}
