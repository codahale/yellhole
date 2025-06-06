use std::{ops::Range, sync::Arc};

use askama::Template;
use axum::{
    Router,
    extract::{Path, Query, State},
    http,
    response::{IntoResponse, Response},
    routing::get,
};
use quick_xml::{
    Writer as XmlWriter,
    events::{BytesDecl, BytesText, Event},
};
use serde::Deserialize;
use time::{Date, Duration, format_description::well_known::Rfc3339};
use tower_http::set_header::SetResponseHeaderLayer;
use url::Url;

use crate::{
    config::Config,
    services::notes::Note,
    web::app::{AppError, AppState, Page},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/note/{:note_id}", get(single))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=31536000,immutable"),
        ))
        .route("/", get(index))
        .route("/atom.xml", get(atom))
        .route("/notes/{:start}", get(week))
        .layer(SetResponseHeaderLayer::if_not_present(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("max-age=300"),
        ))
}

#[derive(Debug, Template)]
#[template(path = "feed.html")]
struct FeedPage {
    config: Arc<Config>,
    notes: Vec<Note>,
    weeks: Vec<Range<Date>>,
}

impl FeedPage {
    fn new(state: AppState, notes: Vec<Note>, weeks: Vec<Range<Date>>) -> FeedPage {
        FeedPage { config: state.config, notes, weeks }
    }
}

mod filters {
    use askama::{Error::Custom, Result};
    use time::{Date, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
    use url::Url;

    use crate::services::notes::Note;

    pub fn to_rfc3339(t: &OffsetDateTime, _: &dyn askama::Values) -> Result<String> {
        t.format(&Rfc3339).map_err(|e| Custom(Box::new(e)))
    }

    pub fn to_local_tz(t: &OffsetDateTime, _: &dyn askama::Values) -> Result<OffsetDateTime> {
        let local = tz::TimeZone::local()
            .map_err(|e| Custom(Box::new(e)))?
            .find_current_local_time_type()
            .map_err(|e| Custom(Box::new(e)))?
            .ut_offset();
        Ok(t.checked_to_offset(
            UtcOffset::from_whole_seconds(local).map_err(|e| Custom(Box::new(e)))?,
        )
        .expect("should convert"))
    }

    pub fn to_note_url(note: &Note, _: &dyn askama::Values, base_url: &Url) -> Result<Url> {
        super::to_note_url(note, base_url).map_err(|e| Custom(Box::new(e)))
    }

    pub fn to_atom_url(base_url: &Url, _: &dyn askama::Values) -> Result<Url> {
        super::to_atom_url(base_url).map_err(|e| Custom(Box::new(e)))
    }

    pub fn to_weekly_url(week: &Date, _: &dyn askama::Values, base_url: &Url) -> Result<Url> {
        base_url
            .join("notes/")
            .and_then(|u| u.join(&week.to_string()))
            .map_err(|e| Custom(Box::new(e)))
    }
}

fn to_atom_url(base_url: &Url) -> Result<Url, url::ParseError> {
    base_url.join("atom.xml")
}

fn to_note_url(note: &Note, base_url: &Url) -> Result<Url, url::ParseError> {
    base_url.join("note/").and_then(|u| u.join(&note.note_id.to_string()))
}

#[derive(Debug, Deserialize)]
struct IndexOpts {
    n: Option<u16>,
}

async fn index(
    State(state): State<AppState>,
    opts: Query<IndexOpts>,
) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let notes = state.notes.most_recent(opts.n.unwrap_or(25)).await?;
    Ok(Page(FeedPage::new(state, notes, weeks)))
}

async fn week(
    State(state): State<AppState>,
    start: Option<Path<Date>>,
) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let start = start.ok_or(AppError::NotFound)?.0;
    let end = start.checked_add(Duration::days(7)).expect("should allow week addition");
    let notes = state.notes.date_range(start..end).await?;
    Ok(Page(FeedPage::new(state, notes, weeks)))
}

async fn single(
    State(state): State<AppState>,
    note_id: Option<Path<String>>,
) -> Result<Page<FeedPage>, AppError> {
    let weeks = state.notes.weeks().await?;
    let note_id = note_id.ok_or(AppError::NotFound)?;
    let notes = vec![state.notes.by_id(&note_id).await?.ok_or(AppError::NotFound)?];
    Ok(Page(FeedPage::new(state, notes, weeks)))
}

async fn atom(State(state): State<AppState>) -> Result<Response, AppError> {
    let notes = state.notes.most_recent(20).await?;
    let atom_url = to_atom_url(&state.config.base_url).expect("should be a valid URL");
    let mut xml = XmlWriter::new(Vec::<u8>::with_capacity(1024));
    xml.write_event(Event::Decl(BytesDecl::new("1.0", None, None))).map_err(anyhow::Error::new)?;
    xml.create_element("feed")
        .with_attributes([
            ("xmlns", "http://www.w3.org/2005/Atom"),
            ("xml:base", atom_url.as_str()),
        ])
        .write_inner_content(|feed| {
            feed.create_element("title")
                .write_text_content(BytesText::new(&state.config.title))?
                .create_element("id")
                .write_text_content(BytesText::new(state.config.base_url.as_str()))?;

            feed.create_element("author")
                .write_inner_content(|author| {
                    author
                        .create_element("name")
                        .write_text_content(BytesText::new(&state.config.author))?;
                    Ok(())
                })?
                .create_element("link")
                .with_attributes([("href", atom_url.as_str()), ("rel", "alternate")])
                .write_empty()?
                .create_element("subtitle")
                .write_text_content(BytesText::new(&state.config.description))?;

            if !notes.is_empty() {
                feed.create_element("updated").write_text_content(BytesText::new(
                    &notes[0].created_at.format(&Rfc3339).expect("should format"),
                ))?;
            }

            for note in notes {
                let url =
                    to_note_url(&note, &state.config.base_url).expect("should be a valid URL");
                feed.create_element("entry").write_inner_content(|entry| {
                    entry
                        .create_element("title")
                        .write_text_content(BytesText::new(&note.note_id.to_string()))?
                        .create_element("id")
                        .write_text_content(BytesText::new(url.as_str()))?
                        .create_element("updated")
                        .write_text_content(BytesText::new(
                            &note.created_at.format(&Rfc3339).expect("should format"),
                        ))?
                        .create_element("link")
                        .with_attributes([("href", url.as_str()), ("rel", "alternate")])
                        .write_empty()?
                        .create_element("content")
                        .with_attribute(("type", "html"))
                        .write_text_content(BytesText::new(&note.to_html()))?;
                    Ok(())
                })?;
            }

            Ok(())
        })
        .map_err(anyhow::Error::new)?;

    Ok(([(http::header::CONTENT_TYPE, atom_xml())], xml.into_inner()).into_response())
}

const fn atom_xml() -> http::HeaderValue {
    http::HeaderValue::from_static("application/atom+xml; charset=utf-8")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use atom_syndication::Feed;
    use reqwest::{StatusCode, header};

    use super::*;
    use crate::test::{TestEnv, TestServer};

    async fn note_fixtures(ts: &TestServer) -> anyhow::Result<()> {
        ts.db.call_unwrap(|conn| {
            conn.execute_batch(r#"
insert into note (note_id, body, created_at)
values ('69b124f0-a4fa-40d0-83f4-06bc4213f3ca', 'It''s a me, _Mario_.', '2022-11-14 18:22:00');

insert into note (note_id, body, created_at)
values ('c1449d6c-6b5b-4ce4-a4d7-98853562fbf1', '# Hello, it is a header.

## A Subheader', '2022-10-14 20:17:31');

insert into note (note_id, body, created_at)
values ('b0a2170c-5e91-42ad-aa1b-dabc3c6ea5b9', 'Ok, I *guess* this is fine.', '2022-09-07 09:43:16');
        "#)
    }).await?;
        Ok(())
    }

    #[tokio::test]
    async fn main() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;
        note_fixtures(&ts).await?;

        let resp = ts.get("/").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[tokio::test]
    async fn atom_feed() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;
        note_fixtures(&ts).await?;

        let resp = ts.get("/atom.xml").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).map(|h| h.as_bytes()),
            Some(atom_xml().as_bytes())
        );

        let feed = Feed::read_from(Cursor::new(&resp.bytes().await?)).expect("should parse");
        assert_eq!(
            feed.entries[0]
                .content()
                .expect("should have content")
                .value()
                .expect("should have a value"),
            "<p>It’s a me, <em>Mario</em>.</p>\n"
        );

        Ok(())
    }

    #[tokio::test]
    async fn weekly_view() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;
        note_fixtures(&ts).await?;

        let resp = ts.get("/notes/2022-10-09").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[tokio::test]
    async fn single_note() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;
        note_fixtures(&ts).await?;

        let resp = ts.get("/note/c1449d6c-6b5b-4ce4-a4d7-98853562fbf1").send().await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.text().await?;
        assert!(body.contains("Hello, it is a header"));

        Ok(())
    }

    #[tokio::test]
    async fn bad_note_id() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;

        let resp = ts.get("/note/not-a-uuid").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[tokio::test]
    async fn missing_note_id() -> Result<(), anyhow::Error> {
        let ts = TestEnv::new().await?.into_server(router()).await?;

        let resp = ts.get("/note/37c615b0-bb55-424d-a813-69e14ca5c20c").send().await?;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }
}
