use std::fmt::Display;

use axum::http::header::LINK;
use axum::http::{HeaderValue, Uri};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;

use super::error::HttpError;
use super::extract::Json;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
const LIMIT: &str = "limit";
const AFTER: &str = "after";

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    #[param(minimum = 1, maximum = 100, default = 50)]
    limit: Option<u32>,
    after: Option<String>,
}

pub struct Page {
    limit: u32,
    after: Option<String>,
}

impl TryFrom<PageQuery> for Page {
    type Error = HttpError;

    fn try_from(query: PageQuery) -> Result<Self, HttpError> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(HttpError::BadRequest(format!(
                "limit: from 1 to {MAX_LIMIT}"
            )));
        }
        Ok(Self {
            limit,
            after: query.after,
        })
    }
}

impl Page {
    pub fn parse_after<K, E: Display>(
        &self,
        parse: impl FnOnce(&str) -> Result<K, E>,
    ) -> Result<Option<K>, HttpError> {
        let after = self.after.as_deref().map(parse).transpose();
        after.map_err(|error| HttpError::BadRequest(format!("{AFTER}: {error}")))
    }

    pub fn rows_to_read(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

pub struct Paged<T> {
    items: Vec<T>,
    next: Option<HeaderValue>,
}

impl<T> Paged<T> {
    pub fn new(mut items: Vec<T>, page: &Page, uri: &Uri, key: impl Fn(&T) -> String) -> Self {
        let limit = usize::try_from(page.limit).unwrap_or(usize::MAX);
        if items.len() <= limit {
            return Self { items, next: None };
        }
        items.truncate(limit);
        let next = items
            .last()
            .and_then(|last| next_link(uri, page, &key(last)));
        Self { items, next }
    }
}

impl<T: Serialize> IntoResponse for Paged<T> {
    fn into_response(self) -> Response {
        let mut response = Json(self.items).into_response();
        if let Some(link) = self.next {
            response.headers_mut().insert(LINK, link);
        }
        response
    }
}

fn next_link(uri: &Uri, page: &Page, after: &str) -> Option<HeaderValue> {
    let kept = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty() && !is_page_pair(pair));
    let page_pairs = [
        format!("{LIMIT}={}", page.limit),
        format!("{AFTER}={}", encode(after)),
    ];
    let query: Vec<String> = kept.map(str::to_owned).chain(page_pairs).collect();
    let link = format!("<{}?{}>; rel=\"next\"", uri.path(), query.join("&"));
    HeaderValue::from_str(&link).ok()
}

fn is_page_pair(pair: &str) -> bool {
    let name = pair.split_once('=').map_or(pair, |(name, _)| name);
    name == LIMIT || name == AFTER
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
