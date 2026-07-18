// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tokio::fs;

use crate::to_unique_file;

const USER_AGENT: &str = "read-flow/0.1 (+https://github.com/read-flow/read-flow)";

// ─── Domain Types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DownloadFormat {
    pub mime_type: String,
    pub href: String,
    pub label: String,
}

impl DownloadFormat {
    pub fn label_from_mime(mime: &str) -> &str {
        match mime {
            "application/epub+zip" => "EPUB",
            "application/pdf" => "PDF",
            "application/x-mobipocket-ebook" | "application/x-mobi8-ebook" => "MOBI",
            "application/x-cbz" => "CBZ",
            "text/plain" | "text/plain; charset=utf-8" => "TXT",
            "text/html" | "text/html; charset=utf-8" => "HTML",
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OnlineBook {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub contributors: Vec<String>,
    pub summary: Option<String>,
    /// Raw HTML collected from `<content type="html">` or serialized from
    /// `<content type="xhtml">`. Preferred over `summary` for display when present.
    pub summary_html: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub identifier: Option<String>,
    pub published: Option<String>,
    pub rights: Option<String>,
    pub subject: Option<String>,
    pub cover_url: Option<String>,
    pub formats: Vec<DownloadFormat>,
    pub catalog_name: String,
}

impl OnlineBook {
    /// Convert OPDS feed metadata to `ExtractedMetadata` for merging into the library DB.
    /// Prefers `publisher` if set; falls back to the first contributor name.
    pub fn to_extracted_metadata(&self) -> crate::scan::metadata::ExtractedMetadata {
        use crate::scan::metadata::ExtractedMetadata;
        let description = self
            .summary
            .clone()
            .or_else(|| self.summary_html.as_deref().map(html_to_plain_text));
        let publisher = self
            .publisher
            .clone()
            .or_else(|| self.contributors.first().cloned());
        ExtractedMetadata {
            title: Some(self.title.clone()),
            subtitle: self.subtitle.clone(),
            authors: self.authors.clone(),
            description,
            language: self.language.clone(),
            publisher,
            identifier: self.identifier.clone(),
            date: self.published.clone(),
            subject: self.subject.clone(),
        }
    }
}

/// Strip HTML tags, returning plain text for use as a description.
fn html_to_plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A resolved, ready-to-use catalog — what [`OpdsClient`] and the REST API
/// consume, regardless of whether it originated from a built-in or a
/// user-configured [`Catalog`].
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OnlineCatalog {
    pub name: String,
    /// OPDS search URL; may contain `{searchTerms}` template placeholder.
    pub search_url: String,
    pub enabled: bool,
}

impl OnlineCatalog {
    pub fn project_gutenberg() -> Self {
        BuiltinCatalogId::ProjectGutenberg.resolve(true)
    }

    pub fn standard_ebooks() -> Self {
        BuiltinCatalogId::StandardEbooks.resolve(true)
    }
}

/// Identifies a built-in catalog. Code owns its display name and search URL —
/// they are never read from or written to `read-flow.toml`, so a built-in's
/// URL can never go stale in a user's config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinCatalogId {
    ProjectGutenberg,
    StandardEbooks,
}

impl BuiltinCatalogId {
    fn display_name(self) -> &'static str {
        match self {
            Self::ProjectGutenberg => "Project Gutenberg",
            Self::StandardEbooks => "Standard Ebooks",
        }
    }

    fn search_url(self) -> &'static str {
        match self {
            Self::ProjectGutenberg => {
                "https://www.gutenberg.org/ebooks/search.opds/?query={searchTerms}"
            }
            Self::StandardEbooks => "https://standardebooks.org/feeds/opds/all?query={searchTerms}",
        }
    }

    pub fn resolve(self, enabled: bool) -> OnlineCatalog {
        OnlineCatalog {
            name: self.display_name().to_string(),
            search_url: self.search_url().to_string(),
            enabled,
        }
    }
}

/// A built-in catalog as stored in config — just an id and whether the user
/// has enabled it. Name and URL are resolved from [`BuiltinCatalogId`].
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct BuiltinCatalog {
    pub id: BuiltinCatalogId,
    pub enabled: bool,
}

/// A user-added OPDS catalog, fully owned by config.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConfiguredCatalog {
    pub name: String,
    pub search_url: String,
    pub enabled: bool,
}

/// A catalog entry as stored in `read-flow.toml`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Catalog {
    Builtin(BuiltinCatalog),
    Configured(ConfiguredCatalog),
}

impl Catalog {
    pub fn resolve(&self) -> OnlineCatalog {
        match self {
            Catalog::Builtin(b) => b.id.resolve(b.enabled),
            Catalog::Configured(c) => OnlineCatalog {
                name: c.name.clone(),
                search_url: c.search_url.clone(),
                enabled: c.enabled,
            },
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Catalog::Builtin(b) => b.enabled,
            Catalog::Configured(c) => c.enabled,
        }
    }
}

/// Resolves stored catalog entries into ready-to-use [`OnlineCatalog`]s.
pub fn resolve_catalogs(catalogs: &[Catalog]) -> Vec<OnlineCatalog> {
    catalogs.iter().map(Catalog::resolve).collect()
}

// ─── Error Type ──────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum OnlineLibraryError {
    #[error("HTTP error: {0}")]
    Http(#[from] Arc<reqwest::Error>),
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("invalid catalog URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("file system error: {0}")]
    Io(#[from] Arc<io::Error>),
    #[error("download URL has no usable filename")]
    NoFilename,
}

impl From<reqwest::Error> for OnlineLibraryError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(Arc::new(e))
    }
}

impl From<io::Error> for OnlineLibraryError {
    fn from(e: io::Error) -> Self {
        Self::Io(Arc::new(e))
    }
}

// ─── Trait ───────────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait OnlineLibraryClient: Send + Sync {
    fn catalog_name(&self) -> &str;
    async fn search(&self, query: &str) -> Result<Vec<OnlineBook>, OnlineLibraryError>;
}

// ─── OPDS Client ─────────────────────────────────────────────────────────────

pub struct OpdsClient {
    catalog: OnlineCatalog,
    client: Client,
}

impl OpdsClient {
    pub fn new(catalog: OnlineCatalog) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default();
        Self { catalog, client }
    }

    async fn fetch_xml(&self, url: &str) -> Result<String, OnlineLibraryError> {
        let response = self
            .client
            .get(url)
            .header(
                reqwest::header::ACCEPT,
                "application/atom+xml, application/xml, */*",
            )
            .send()
            .await?
            .error_for_status()?;
        Ok(response.text().await?)
    }

    /// Fetch and parse an OPDS page at `url`, resolving stubs and returning any
    /// `rel="next"` link alongside the collected books.
    async fn fetch_page_at_url(
        &self,
        url: &str,
    ) -> Result<(Vec<OnlineBook>, Option<String>), OnlineLibraryError> {
        let xml = self.fetch_xml(url).await?;
        let FeedResult {
            mut books,
            stubs,
            next_url,
        } = parse_opds_feed_full(&xml, &self.catalog.name)?;

        if !stubs.is_empty() {
            let client = self.client.clone();
            let catalog_name = self.catalog.name.clone();
            let stub_futures = stubs.into_iter().map(|stub| {
                let client = client.clone();
                let catalog_name = catalog_name.clone();
                let sub_url = resolve_url(url, &stub.subsection_url);
                async move {
                    let sub_url = sub_url?;
                    let response = client
                        .get(&sub_url)
                        .header(
                            reqwest::header::ACCEPT,
                            "application/atom+xml, application/xml, */*",
                        )
                        .send()
                        .await?
                        .error_for_status()?;
                    let xml = response.text().await?;
                    let FeedResult { books, .. } = parse_opds_feed_full(&xml, &catalog_name)?;
                    Ok::<Vec<OnlineBook>, OnlineLibraryError>(books)
                }
            });
            let results = futures::future::join_all(stub_futures).await;
            for result in results {
                match result {
                    Ok(mut sub_books) => books.append(&mut sub_books),
                    Err(e) => tracing::warn!("OPDS sub-page fetch failed: {e}"),
                }
            }
        }

        let resolved_next = next_url.and_then(|u| resolve_url(url, &u).ok());
        Ok((merge_gutenberg_editions(books), resolved_next))
    }

    /// Like `search`, but also returns the `rel="next"` URL from the feed if present.
    pub async fn search_with_next(
        &self,
        query: &str,
    ) -> Result<(Vec<OnlineBook>, Option<String>), OnlineLibraryError> {
        let url = build_search_url(&self.catalog.search_url, query)?;
        self.fetch_page_at_url(&url).await
    }

    /// Fetch the next page of results from a URL previously returned as `rel="next"`.
    pub async fn fetch_next_page(
        &self,
        url: &str,
    ) -> Result<(Vec<OnlineBook>, Option<String>), OnlineLibraryError> {
        self.fetch_page_at_url(url).await
    }
}

#[async_trait::async_trait]
impl OnlineLibraryClient for OpdsClient {
    fn catalog_name(&self) -> &str {
        &self.catalog.name
    }

    async fn search(&self, query: &str) -> Result<Vec<OnlineBook>, OnlineLibraryError> {
        Ok(self.search_with_next(query).await?.0)
    }
}

// ─── Pure Functions (testable) ────────────────────────────────────────────────

/// Replace `{searchTerms}` in `template` with a percent-encoded `query`.
/// If the template has no placeholder, appends `query` as a `q=` parameter.
pub fn build_search_url(template: &str, query: &str) -> Result<String, OnlineLibraryError> {
    if query.is_empty() {
        return Ok(template.to_string());
    }
    if template.contains("{searchTerms}") {
        let encoded = percent_encode(query);
        Ok(template.replace("{searchTerms}", &encoded))
    } else {
        let mut url = url::Url::parse(template)?;
        url.query_pairs_mut().append_pair("q", query);
        Ok(url.to_string())
    }
}

/// An entry with a navigation (`rel="subsection"`) link but no acquisition links.
/// Gutenberg's search feed is two-level: search results list stubs pointing to
/// individual book pages that carry the actual download links.
struct BookStub {
    subsection_url: String,
}

struct FeedResult {
    books: Vec<OnlineBook>,
    stubs: Vec<BookStub>,
    next_url: Option<String>,
}

/// Parse an OPDS 1.x Atom feed XML string into a list of [`OnlineBook`]s.
/// Only entries that have at least one acquisition link are returned.
pub fn parse_opds_feed(
    xml: &str,
    catalog_name: &str,
) -> Result<Vec<OnlineBook>, OnlineLibraryError> {
    parse_opds_feed_full(xml, catalog_name).map(|r| r.books)
}

fn parse_opds_feed_full(xml: &str, catalog_name: &str) -> Result<FeedResult, OnlineLibraryError> {
    let mut reader = Reader::from_reader(xml.as_bytes());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut books: Vec<OnlineBook> = Vec::new();
    let mut stubs: Vec<BookStub> = Vec::new();
    let mut feed_next_url: Option<String> = None;

    // Per-entry state
    let mut in_entry = false;
    let mut in_author = false;
    let mut in_contributor = false;

    // Tracks which field we're collecting text for
    #[derive(Debug, PartialEq)]
    enum Collecting {
        Title,
        Subtitle,
        Id,
        Summary,
        AuthorName,
        ContributorName,
        Language,
        Publisher,
        Identifier,
        Published,
        Rights,
        Subject,
    }
    let mut collecting: Option<Collecting> = None;
    let mut cur_text = String::new();

    // Current entry being built
    let mut cur_id = String::new();
    let mut cur_title = String::new();
    let mut cur_subtitle: Option<String> = None;
    let mut cur_authors: Vec<String> = Vec::new();
    let mut cur_contributors: Vec<String> = Vec::new();
    let mut cur_summary: Option<String> = None;
    let mut cur_summary_html: Option<String> = None;
    let mut cur_language: Option<String> = None;
    let mut cur_publisher: Option<String> = None;
    let mut cur_identifier: Option<String> = None;
    let mut cur_published: Option<String> = None;
    let mut cur_rights: Option<String> = None;
    let mut cur_subject: Option<String> = None;
    let mut cur_cover_url: Option<String> = None;
    let mut cur_formats: Vec<DownloadFormat> = Vec::new();
    let mut cur_subsection_url: Option<String> = None;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,

            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"entry" => {
                        in_entry = true;
                        cur_id.clear();
                        cur_title.clear();
                        cur_subtitle = None;
                        cur_authors.clear();
                        cur_contributors.clear();
                        cur_summary = None;
                        cur_summary_html = None;
                        cur_language = None;
                        cur_publisher = None;
                        cur_identifier = None;
                        cur_published = None;
                        cur_rights = None;
                        cur_subject = None;
                        cur_cover_url = None;
                        cur_formats.clear();
                        cur_subsection_url = None;
                        collecting = None;
                        cur_text.clear();
                    }
                    b"title" if in_entry => {
                        collecting = Some(Collecting::Title);
                        cur_text.clear();
                    }
                    b"subtitle" if in_entry => {
                        collecting = Some(Collecting::Subtitle);
                        cur_text.clear();
                    }
                    b"id" if in_entry => {
                        collecting = Some(Collecting::Id);
                        cur_text.clear();
                    }
                    b"summary" | b"content" if in_entry => {
                        let content_type = get_attr(e, b"type").unwrap_or_default();
                        match content_type.as_str() {
                            "html" => {
                                // Inner loop consumes </content> and returns the decoded HTML.
                                let raw = collect_html_content(&mut reader);
                                if !raw.trim().is_empty() {
                                    cur_summary_html = Some(raw);
                                }
                            }
                            "xhtml" => {
                                // Collect inner XHTML nodes as an HTML string.
                                let raw = collect_xhtml_inner(&mut reader);
                                if !raw.trim().is_empty() {
                                    cur_summary_html = Some(raw);
                                }
                                // Inner loop consumed </content>; skip outer End handling.
                            }
                            _ => {
                                // type="text" or no type → plain text
                                collecting = Some(Collecting::Summary);
                                cur_text.clear();
                            }
                        }
                    }
                    b"author" if in_entry => {
                        in_author = true;
                    }
                    b"contributor" if in_entry => {
                        in_contributor = true;
                    }
                    b"name" if in_author => {
                        collecting = Some(Collecting::AuthorName);
                        cur_text.clear();
                    }
                    b"name" if in_contributor => {
                        collecting = Some(Collecting::ContributorName);
                        cur_text.clear();
                    }
                    // Dublin Core / OPDS extension elements (dc:language, dc:publisher, etc.)
                    // local_name() strips the namespace prefix, so dc:X and X both match.
                    b"language" if in_entry => {
                        collecting = Some(Collecting::Language);
                        cur_text.clear();
                    }
                    b"publisher" if in_entry => {
                        collecting = Some(Collecting::Publisher);
                        cur_text.clear();
                    }
                    b"identifier" if in_entry => {
                        collecting = Some(Collecting::Identifier);
                        cur_text.clear();
                    }
                    b"published" | b"date" if in_entry => {
                        collecting = Some(Collecting::Published);
                        cur_text.clear();
                    }
                    b"rights" if in_entry => {
                        collecting = Some(Collecting::Rights);
                        cur_text.clear();
                    }
                    b"subject" if in_entry => {
                        collecting = Some(Collecting::Subject);
                        cur_text.clear();
                    }
                    b"link" if in_entry => {
                        process_link(
                            e,
                            &mut cur_formats,
                            &mut cur_cover_url,
                            &mut cur_subsection_url,
                        );
                    }
                    b"link" if get_attr(e, b"rel").as_deref() == Some("next") => {
                        feed_next_url = get_attr(e, b"href");
                    }
                    _ => {
                        let t = std::str::from_utf8(local).unwrap();
                        tracing::debug!("ignoring unhandled tag: `{t}`");
                    }
                }
            }

            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"link" {
                    if in_entry {
                        process_link(
                            e,
                            &mut cur_formats,
                            &mut cur_cover_url,
                            &mut cur_subsection_url,
                        );
                    } else if get_attr(e, b"rel").as_deref() == Some("next") {
                        feed_next_url = get_attr(e, b"href");
                    }
                }
            }

            Ok(Event::Text(ref e)) => {
                if collecting.is_some()
                    && let Ok(t) = e.xml_content(XmlVersion::Explicit1_1)
                {
                    cur_text.push_str(&t);
                }
            }

            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"entry" if in_entry => {
                        if !cur_formats.is_empty() && !cur_title.is_empty() {
                            books.push(OnlineBook {
                                id: cur_id.clone(),
                                title: cur_title.clone(),
                                subtitle: cur_subtitle.clone(),
                                authors: cur_authors.clone(),
                                contributors: cur_contributors.clone(),
                                summary: cur_summary.clone(),
                                summary_html: cur_summary_html.clone(),
                                language: cur_language.clone(),
                                publisher: cur_publisher.clone(),
                                identifier: cur_identifier.clone(),
                                published: cur_published.clone(),
                                rights: cur_rights.clone(),
                                subject: cur_subject.clone(),
                                cover_url: cur_cover_url.clone(),
                                formats: cur_formats.clone(),
                                catalog_name: catalog_name.to_string(),
                            });
                        } else if let Some(sub_url) = cur_subsection_url.clone()
                            && !cur_title.is_empty()
                        {
                            stubs.push(BookStub {
                                subsection_url: sub_url,
                            });
                        }
                        in_entry = false;
                        in_author = false;
                        in_contributor = false;
                        collecting = None;
                    }
                    b"author" if in_author => {
                        in_author = false;
                    }
                    b"contributor" if in_contributor => {
                        in_contributor = false;
                    }
                    b"title" if in_entry => {
                        if matches!(collecting, Some(Collecting::Title)) {
                            cur_title = cur_text.trim().to_string();
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"subtitle" if in_entry => {
                        if matches!(collecting, Some(Collecting::Subtitle)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_subtitle = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"id" if in_entry => {
                        if matches!(collecting, Some(Collecting::Id)) {
                            cur_id = cur_text.trim().to_string();
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"summary" | b"content" if in_entry => {
                        if let Some(Collecting::Summary) = collecting {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_summary = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"name" if in_author => {
                        if matches!(collecting, Some(Collecting::AuthorName)) {
                            let name = cur_text.trim().to_string();
                            if !name.is_empty() {
                                cur_authors.push(name);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"name" if in_contributor => {
                        if matches!(collecting, Some(Collecting::ContributorName)) {
                            let name = cur_text.trim().to_string();
                            if !name.is_empty() {
                                cur_contributors.push(name);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"language" if in_entry => {
                        if matches!(collecting, Some(Collecting::Language)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_language = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"publisher" if in_entry => {
                        if matches!(collecting, Some(Collecting::Publisher)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_publisher = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"identifier" if in_entry => {
                        if matches!(collecting, Some(Collecting::Identifier)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_identifier = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"published" | b"date" if in_entry => {
                        if matches!(collecting, Some(Collecting::Published)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_published = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"rights" if in_entry => {
                        if matches!(collecting, Some(Collecting::Rights)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_rights = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    b"subject" if in_entry => {
                        if matches!(collecting, Some(Collecting::Subject)) {
                            let s = cur_text.trim().to_string();
                            if !s.is_empty() {
                                cur_subject = Some(s);
                            }
                            cur_text.clear();
                            collecting = None;
                        }
                    }
                    _ => {}
                }
            }

            Err(e) => return Err(OnlineLibraryError::Xml(e.to_string())),
            _ => {}
        }
    }

    Ok(FeedResult {
        books,
        stubs,
        next_url: feed_next_url,
    })
}

// ─── Gutenberg edition merging ───────────────────────────────────────────────

/// Extract the base id from a Gutenberg variant entry id.
/// `urn:gutenberg:2701:3` → `urn:gutenberg:2701`; anything else → `None`.
fn gutenberg_base_id(id: &str) -> Option<String> {
    let rest = id.strip_prefix("urn:gutenberg:")?;
    let (book, variant) = rest.split_once(':')?;
    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (is_number(book) && is_number(variant)).then(|| format!("urn:gutenberg:{book}"))
}

/// Gutenberg book pages list one `<entry>` per edition variant of the same book
/// (e.g. `urn:gutenberg:2701:2` without images, `urn:gutenberg:2701:3` with
/// images). Merge entries sharing a base id into a single book whose format
/// list is the union of all variants; the per-format labels ("EPUB (no images,
/// older E-readers)", "EPUB3 (E-readers incl. Send-to-Kindle)", …) keep the
/// variants distinguishable as download options. Entries whose id does not
/// match the Gutenberg pattern pass through unchanged, so other catalogs are
/// unaffected.
pub fn merge_gutenberg_editions(books: Vec<OnlineBook>) -> Vec<OnlineBook> {
    let mut merged: Vec<OnlineBook> = Vec::with_capacity(books.len());
    let mut index_by_base: HashMap<String, usize> = HashMap::new();
    for book in books {
        match gutenberg_base_id(&book.id) {
            Some(base) => {
                let book = strip_gutenberg_edition_note(book);
                match index_by_base.get(&base) {
                    Some(&i) => merge_book_into(&mut merged[i], book),
                    None => {
                        index_by_base.insert(base.clone(), merged.len());
                        merged.push(OnlineBook { id: base, ..book });
                    }
                }
            }
            None => merged.push(book),
        }
    }
    merged
}

/// Per-variant edition notes Gutenberg prepends to each entry's description.
/// After merging variants into one book these no longer describe the result,
/// so they are removed from both the plain and HTML summaries.
const GUTENBERG_EDITION_NOTES: [&str; 2] = [
    "This edition had all images removed.",
    "This edition has images.",
];

fn strip_gutenberg_edition_note(book: OnlineBook) -> OnlineBook {
    let strip = |text: String, wrap: fn(&str) -> String| {
        GUTENBERG_EDITION_NOTES
            .iter()
            .fold(text, |acc, note| acc.replace(&wrap(note), ""))
            .trim()
            .to_string()
    };
    let summary = book
        .summary
        .map(|s| strip(s, str::to_string))
        .filter(|s| !s.is_empty());
    let summary_html = book
        .summary_html
        .map(|s| strip(s, |note| format!("<p>{note}</p>")))
        .filter(|s| !html_to_plain_text(s).is_empty());
    OnlineBook {
        summary,
        summary_html,
        ..book
    }
}

/// Fold `other` into `target`: append unseen formats (by href) and fill in
/// metadata fields that the earlier variant left empty.
fn merge_book_into(target: &mut OnlineBook, other: OnlineBook) {
    let known: Vec<String> = target.formats.iter().map(|f| f.href.clone()).collect();
    target.formats.extend(
        other
            .formats
            .into_iter()
            .filter(|f| !known.contains(&f.href)),
    );
    target.subtitle = target.subtitle.take().or(other.subtitle);
    target.summary = target.summary.take().or(other.summary);
    target.summary_html = target.summary_html.take().or(other.summary_html);
    target.language = target.language.take().or(other.language);
    target.publisher = target.publisher.take().or(other.publisher);
    target.identifier = target.identifier.take().or(other.identifier);
    target.published = target.published.take().or(other.published);
    target.rights = target.rights.take().or(other.rights);
    target.subject = target.subject.take().or(other.subject);
    target.cover_url = target.cover_url.take().or(other.cover_url);
    if target.authors.is_empty() {
        target.authors = other.authors;
    }
    if target.contributors.is_empty() {
        target.contributors = other.contributors;
    }
}

// ─── Download ────────────────────────────────────────────────────────────────

/// Download a book format to `download_folder` and return the saved path.
pub async fn download_book(
    format: &DownloadFormat,
    title: &str,
    download_folder: &Path,
) -> Result<PathBuf, OnlineLibraryError> {
    let client = Client::new();
    let response = client.get(&format.href).send().await?;

    // Always use the MIME type for the extension — URL-derived extensions
    // (e.g. `.epub3.images` from Gutenberg) are not reliable type indicators.
    let ext = mime_to_extension(&format.mime_type);
    let stem = sanitize_title(title);
    let filename = format!("{stem}.{ext}");

    let mut target = download_folder.join(&filename);
    to_unique_file(&mut target, ext);

    let mut file = fs::File::create(&target).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        tokio::io::copy(&mut chunk?.as_ref(), &mut file).await?;
    }

    Ok(target)
}

pub async fn fetch_cover_bytes(url: &str) -> Result<(Vec<u8>, String), OnlineLibraryError> {
    let client = Client::new();
    let resp = client.get(url).send().await?;
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "image/jpeg".to_string());
    let bytes = resp.bytes().await?.to_vec();
    Ok((bytes, mime))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a book title into a safe filename stem.
/// e.g. "Moby Dick; Or, The Whale" → "moby-dick-or-the-whale"
fn sanitize_title(title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let parts: Vec<&str> = slug.split('-').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        "book".to_string()
    } else {
        parts.join("-")
    }
}

fn resolve_url(base: &str, href: &str) -> Result<String, OnlineLibraryError> {
    if href.starts_with("http://") || href.starts_with("https://") {
        Ok(href.to_string())
    } else {
        let resolved = url::Url::parse(base)?.join(href)?;
        Ok(resolved.to_string())
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

fn process_link(
    e: &quick_xml::events::BytesStart<'_>,
    formats: &mut Vec<DownloadFormat>,
    cover_url: &mut Option<String>,
    subsection_url: &mut Option<String>,
) {
    let mut rel = String::new();
    let mut href = String::new();
    let mut mime_type = String::new();
    let mut title_attr = String::new();

    for attr in e.attributes().flatten() {
        let attr_key = attr.key.as_ref().to_vec();
        let key = local_name(&attr_key);
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match key {
            b"rel" => rel = val,
            b"href" => href = val,
            b"type" => mime_type = val,
            b"title" => title_attr = val,
            _ => {}
        }
    }

    if href.is_empty() {
        return;
    }

    if rel == "http://opds-spec.org/acquisition"
        || rel.starts_with("http://opds-spec.org/acquisition/")
    {
        let label = if title_attr.is_empty() {
            DownloadFormat::label_from_mime(&mime_type).to_string()
        } else {
            title_attr
        };
        formats.push(DownloadFormat {
            mime_type,
            href,
            label,
        });
    } else if rel == "http://opds-spec.org/image" || rel == "http://opds-spec.org/thumbnail" {
        if cover_url.is_none() {
            *cover_url = Some(href);
        }
    } else if rel == "subsection" && subsection_url.is_none() {
        *subsection_url = Some(href);
    }
}

fn get_attr(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if local_name(a.key.as_ref()) == name {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            b => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    encoded
}

fn mime_to_extension(mime: &str) -> &str {
    match mime {
        "application/epub+zip" => "epub",
        "application/pdf" => "pdf",
        "application/x-mobipocket-ebook" | "application/x-mobi8-ebook" => "mobi",
        "text/plain" | "text/plain; charset=utf-8" => "txt",
        "text/html" | "text/html; charset=utf-8" => "html",
        _ => "bin",
    }
}

// ─── HTML / XHTML content collectors ────────────────────────────────────────

/// Reads events from `reader` starting just after `<content type="html">`,
/// decodes XML entity references (e.g. `&lt;` → `<`), and returns the plain
/// HTML string. Consumes the matching `</content>` end tag before returning.
/// Temporarily disables text trimming so intra-tag spaces are preserved.
fn collect_html_content(reader: &mut Reader<&[u8]>) -> String {
    let trim_start = reader.config().trim_text_start;
    let trim_end = reader.config().trim_text_end;
    reader.config_mut().trim_text_start = false;
    reader.config_mut().trim_text_end = false;

    let mut buf = Vec::new();
    let mut result = String::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.decode() {
                    result.push_str(&t);
                }
            }
            Ok(Event::GeneralRef(ref e)) => {
                if let Some(ch) = resolve_predefined_entity(e) {
                    result.push(ch);
                }
            }
            Ok(Event::End(_)) | Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    reader.config_mut().trim_text_start = trim_start;
    reader.config_mut().trim_text_end = trim_end;
    result
}

/// Resolve the five predefined XML entities and numeric character references.
/// Returns `None` for unknown named entity references.
fn resolve_predefined_entity(e: &quick_xml::events::BytesRef<'_>) -> Option<char> {
    let name = e.decode().ok()?;
    match name.as_ref() {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => e.resolve_char_ref().ok()?,
    }
}

/// Reads events from `reader` starting just after `<content type="xhtml">` and
/// serialises the inner XML nodes to an HTML string. Consumes the matching
/// `</content>` end tag before returning, so the outer event loop won't see it.
fn collect_xhtml_inner(reader: &mut Reader<&[u8]>) -> String {
    let mut buf = Vec::new();
    let mut depth: u32 = 1; // already inside <content>
    let mut raw = String::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let qname = e.name();
                let local = local_name(qname.as_ref());
                if let Ok(name) = std::str::from_utf8(local) {
                    raw.push('<');
                    raw.push_str(name);
                    serialize_attrs_to_html(e, &mut raw);
                    raw.push('>');
                }
            }
            Ok(Event::End(ref e)) => {
                depth -= 1;
                if depth == 0 {
                    break; // consumed </content>
                }
                let qname = e.name();
                let local = local_name(qname.as_ref());
                if let Ok(name) = std::str::from_utf8(local) {
                    raw.push_str("</");
                    raw.push_str(name);
                    raw.push('>');
                }
            }
            Ok(Event::Empty(ref e)) => {
                let qname = e.name();
                let local = local_name(qname.as_ref());
                if let Ok(name) = std::str::from_utf8(local) {
                    raw.push('<');
                    raw.push_str(name);
                    serialize_attrs_to_html(e, &mut raw);
                    raw.push_str("/>");
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.xml_content(XmlVersion::Explicit1_1)
                    && !t.trim().is_empty()
                {
                    raw.push_str(&t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    raw
}

/// Write HTML attributes from a start/empty tag, skipping namespace declarations.
fn serialize_attrs_to_html(e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    for attr in e.attributes().flatten() {
        let attr_key = attr.key.as_ref().to_vec();
        if attr_key.starts_with(b"xmlns") {
            continue; // skip xmlns="..." and xmlns:foo="..."
        }
        let local = local_name(&attr_key);
        if let Ok(name) = std::str::from_utf8(local) {
            let val = String::from_utf8_lossy(&attr.value).into_owned();
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            for c in val.chars() {
                if c == '"' {
                    out.push_str("&quot;");
                } else {
                    out.push(c);
                }
            }
            out.push('"');
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_opds_feed ──────────────────────────────────────────────────────

    #[test]
    fn parse_feed_returns_books_with_formats() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opds="http://opds-spec.org/2010/catalog">
  <entry>
    <id>urn:gutenberg:1342</id>
    <title>Pride and Prejudice</title>
    <author><name>Austen, Jane</name></author>
    <summary>A classic novel.</summary>
    <link rel="http://opds-spec.org/image" href="/covers/1342.jpg" type="image/jpeg"/>
    <link rel="http://opds-spec.org/acquisition" href="/files/1342.epub" type="application/epub+zip"/>
    <link rel="http://opds-spec.org/acquisition" href="/files/1342.pdf" type="application/pdf"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Pride and Prejudice");
        assert_eq!(books[0].authors, vec!["Austen, Jane"]);
        assert_eq!(books[0].formats.len(), 2);
        assert_eq!(books[0].cover_url.as_deref(), Some("/covers/1342.jpg"));
        assert_eq!(books[0].catalog_name, "Test");
        assert_eq!(books[0].summary.as_deref(), Some("A classic novel."));
    }

    #[test]
    fn parse_feed_empty_returns_empty_vec() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom"></feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert!(books.is_empty());
    }

    #[test]
    fn parse_feed_skips_navigation_only_entries() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:nav:1</id>
    <title>Browse by subject</title>
    <link rel="subsection" href="/subjects" type="application/atom+xml"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert!(books.is_empty(), "navigation entries should be skipped");
    }

    #[test]
    fn parse_feed_collects_multiple_authors() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:1</id>
    <title>Co-authored Book</title>
    <author><name>Smith, Alice</name></author>
    <author><name>Jones, Bob</name></author>
    <link rel="http://opds-spec.org/acquisition" href="/book.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].authors, vec!["Smith, Alice", "Jones, Bob"]);
    }

    #[test]
    fn parse_feed_malformed_xml_returns_error() {
        let xml = "<feed><entry><title>Unclosed";
        // Quick-xml may return Ok with partial results or Err depending on where it fails.
        // What matters is we don't panic.
        let _ = parse_opds_feed(xml, "Test");
    }

    #[test]
    fn parse_feed_handles_namespace_prefixed_title() {
        let xml = r#"<?xml version="1.0"?>
<atom:feed xmlns:atom="http://www.w3.org/2005/Atom">
  <atom:entry>
    <atom:id>urn:test:2</atom:id>
    <atom:title>Prefixed Title</atom:title>
    <atom:author><atom:name>Doe, John</atom:name></atom:author>
    <atom:link rel="http://opds-spec.org/acquisition" href="/book.epub" type="application/epub+zip"/>
  </atom:entry>
</atom:feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Prefixed Title");
    }

    #[test]
    fn parse_feed_acquisition_indirect_rel() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:3</id>
    <title>Indirect Acquisition Book</title>
    <link rel="http://opds-spec.org/acquisition/borrow" href="/borrow.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].formats.len(), 1);
    }

    #[test]
    fn parse_feed_prefers_plain_text_summary_over_html_content() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:1</id>
    <title>A Book</title>
    <summary type="text">Short plain summary.</summary>
    <content type="html">&lt;p&gt;Long &lt;b&gt;HTML&lt;/b&gt; description.&lt;/p&gt;</content>
    <link rel="http://opds-spec.org/acquisition" href="/book.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books[0].summary.as_deref(), Some("Short plain summary."));
    }

    #[test]
    fn parse_feed_collects_html_content_into_summary_html() {
        // <content type="html"> → summary stays None, summary_html captures the HTML
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:2</id>
    <title>A Book</title>
    <content type="html">&lt;p&gt;A &lt;i&gt;classic&lt;/i&gt; novel.&lt;/p&gt;</content>
    <link rel="http://opds-spec.org/acquisition" href="/book.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books[0].summary, None);
        assert_eq!(
            books[0].summary_html.as_deref(),
            Some("<p>A <i>classic</i> novel.</p>")
        );
    }

    #[test]
    fn parse_feed_collects_xhtml_content_into_summary_html() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:3</id>
    <title>A Book</title>
    <content type="xhtml">
      <div xmlns="http://www.w3.org/1999/xhtml"><p>First.</p><p>Second.</p></div>
    </content>
    <link rel="http://opds-spec.org/acquisition" href="/book.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books[0].summary, None);
        assert_eq!(
            books[0].summary_html.as_deref(),
            Some("<div><p>First.</p><p>Second.</p></div>")
        );
    }

    #[test]
    fn parse_feed_collects_dc_metadata() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:dc="http://purl.org/dc/elements/1.1/">
  <entry>
    <id>urn:test:dc1</id>
    <title>Moby Dick</title>
    <author><name>Melville, Herman</name></author>
    <contributor><name>Penguin Classics</name></contributor>
    <dc:language>en</dc:language>
    <dc:publisher>Penguin Classics</dc:publisher>
    <dc:identifier>isbn:9780142437247</dc:identifier>
    <published>1851-10-18</published>
    <rights>Public domain</rights>
    <dc:subject>Fiction</dc:subject>
    <link rel="http://opds-spec.org/acquisition" href="/moby.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books.len(), 1);
        let b = &books[0];
        assert_eq!(b.language.as_deref(), Some("en"));
        assert_eq!(b.publisher.as_deref(), Some("Penguin Classics"));
        assert_eq!(b.identifier.as_deref(), Some("isbn:9780142437247"));
        assert_eq!(b.published.as_deref(), Some("1851-10-18"));
        assert_eq!(b.rights.as_deref(), Some("Public domain"));
        assert_eq!(b.subject.as_deref(), Some("Fiction"));
        assert_eq!(b.contributors, vec!["Penguin Classics"]);
    }

    #[test]
    fn parse_feed_collects_subtitle() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:sub1</id>
    <title>Moby Dick</title>
    <subtitle>Or, The Whale</subtitle>
    <link rel="http://opds-spec.org/acquisition" href="/moby.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let books = parse_opds_feed(xml, "Test").unwrap();
        assert_eq!(books[0].subtitle.as_deref(), Some("Or, The Whale"));
    }

    #[test]
    fn to_extracted_metadata_maps_fields() {
        let book = OnlineBook {
            id: "urn:test:1".into(),
            title: "Moby Dick".into(),
            subtitle: Some("Or, The Whale".into()),
            authors: vec!["Melville, Herman".into()],
            contributors: vec!["Penguin Classics".into()],
            summary: Some("A seafaring tale.".into()),
            summary_html: None,
            language: Some("en".into()),
            publisher: Some("Penguin".into()),
            identifier: Some("isbn:123".into()),
            published: Some("1851-10-18".into()),
            rights: Some("Public domain".into()),
            subject: Some("Fiction".into()),
            cover_url: None,
            formats: vec![],
            catalog_name: "Test".into(),
        };
        let meta = book.to_extracted_metadata();
        assert_eq!(meta.title.as_deref(), Some("Moby Dick"));
        assert_eq!(meta.subtitle.as_deref(), Some("Or, The Whale"));
        assert_eq!(meta.authors, vec!["Melville, Herman"]);
        assert_eq!(meta.description.as_deref(), Some("A seafaring tale."));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert_eq!(meta.publisher.as_deref(), Some("Penguin"));
        assert_eq!(meta.identifier.as_deref(), Some("isbn:123"));
        assert_eq!(meta.date.as_deref(), Some("1851-10-18"));
        assert_eq!(meta.subject.as_deref(), Some("Fiction"));
    }

    #[test]
    fn to_extracted_metadata_falls_back_to_contributor_as_publisher() {
        let book = OnlineBook {
            id: "urn:test:2".into(),
            title: "Book".into(),
            subtitle: None,
            authors: vec![],
            contributors: vec!["Publisher Co".into()],
            summary: None,
            summary_html: None,
            language: None,
            publisher: None, // no explicit publisher
            identifier: None,
            published: None,
            rights: None,
            subject: None,
            cover_url: None,
            formats: vec![],
            catalog_name: "Test".into(),
        };
        let meta = book.to_extracted_metadata();
        assert_eq!(meta.publisher.as_deref(), Some("Publisher Co"));
    }

    #[test]
    fn parse_feed_full_collects_subsection_stubs() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>https://www.gutenberg.org/ebooks/2701.opds</id>
    <title>Moby Dick; Or, The Whale</title>
    <content type="text">Herman Melville</content>
    <link type="application/atom+xml;profile=opds-catalog" rel="subsection" href="/ebooks/2701.opds"/>
  </entry>
</feed>"#;
        let result = parse_opds_feed_full(xml, "Test").unwrap();
        assert!(result.books.is_empty(), "no acquisition links → no book");
        assert_eq!(result.stubs.len(), 1);
        assert_eq!(result.stubs[0].subsection_url, "/ebooks/2701.opds");
    }

    #[test]
    fn resolve_url_resolves_relative_path() {
        let resolved = resolve_url(
            "https://www.gutenberg.org/ebooks/search.opds/?query=moby",
            "/ebooks/2701.opds",
        )
        .unwrap();
        assert_eq!(resolved, "https://www.gutenberg.org/ebooks/2701.opds");
    }

    #[test]
    fn resolve_url_leaves_absolute_unchanged() {
        let href = "https://example.com/book.opds";
        let resolved = resolve_url("https://base.example.com/feed", href).unwrap();
        assert_eq!(resolved, href);
    }

    // ── build_search_url ─────────────────────────────────────────────────────

    #[test]
    fn build_search_url_replaces_search_terms() {
        let url = build_search_url(
            "https://www.gutenberg.org/ebooks/search.opds?query={searchTerms}",
            "moby dick",
        )
        .unwrap();
        assert!(
            url.contains("moby%20dick"),
            "expected percent-encoded space, got: {url}"
        );
        assert!(!url.contains("{searchTerms}"), "placeholder not replaced");
    }

    #[test]
    fn build_search_url_encodes_special_chars() {
        let url = build_search_url(
            "https://example.com/search?q={searchTerms}",
            "C++ programming",
        )
        .unwrap();
        assert!(url.contains("%2B%2B"), "'+' should be encoded");
    }

    #[test]
    fn build_search_url_appends_param_when_no_template() {
        let url = build_search_url("https://example.com/opds/all", "hemingway").unwrap();
        assert!(
            url.contains("q=hemingway"),
            "query param should be appended"
        );
    }

    #[test]
    fn build_search_url_empty_query_returns_template_unchanged() {
        let template = "https://example.com/search?q={searchTerms}";
        let url = build_search_url(template, "").unwrap();
        assert_eq!(url, template);
    }

    // ── DownloadFormat::label_from_mime ──────────────────────────────────────

    #[test]
    fn label_from_known_mimes() {
        assert_eq!(
            DownloadFormat::label_from_mime("application/epub+zip"),
            "EPUB"
        );
        assert_eq!(DownloadFormat::label_from_mime("application/pdf"), "PDF");
        assert_eq!(
            DownloadFormat::label_from_mime("application/x-mobipocket-ebook"),
            "MOBI"
        );
        assert_eq!(DownloadFormat::label_from_mime("text/plain"), "TXT");
    }

    #[test]
    fn label_from_unknown_mime_returns_mime_itself() {
        let mime = "application/x-custom-format";
        assert_eq!(DownloadFormat::label_from_mime(mime), mime);
    }

    // ── sanitize_title ───────────────────────────────────────────────────────

    #[test]
    fn sanitize_title_basic() {
        assert_eq!(sanitize_title("Moby Dick"), "moby-dick");
    }

    #[test]
    fn sanitize_title_with_punctuation() {
        assert_eq!(
            sanitize_title("Moby Dick; Or, The Whale"),
            "moby-dick-or-the-whale"
        );
    }

    #[test]
    fn sanitize_title_collapses_consecutive_separators() {
        assert_eq!(sanitize_title("A  Book -- Title"), "a-book-title");
    }

    #[test]
    fn sanitize_title_empty_falls_back_to_book() {
        assert_eq!(sanitize_title(""), "book");
        assert_eq!(sanitize_title("---"), "book");
    }

    // ── merge_gutenberg_editions ─────────────────────────────────────────────

    fn book(id: &str, formats: Vec<DownloadFormat>) -> OnlineBook {
        OnlineBook {
            id: id.into(),
            title: "Moby Dick".into(),
            subtitle: None,
            authors: vec!["Melville, Herman".into()],
            contributors: vec![],
            summary: None,
            summary_html: None,
            language: None,
            publisher: None,
            identifier: None,
            published: None,
            rights: None,
            subject: None,
            cover_url: None,
            formats,
            catalog_name: "Project Gutenberg".into(),
        }
    }

    fn format(label: &str, href: &str) -> DownloadFormat {
        DownloadFormat {
            mime_type: "application/epub+zip".into(),
            href: href.into(),
            label: label.into(),
        }
    }

    #[test]
    fn merge_combines_gutenberg_variants_of_same_book() {
        let no_images = book(
            "urn:gutenberg:2701:2",
            vec![format(
                "EPUB (no images, older E-readers)",
                "/2701.epub.noimages",
            )],
        );
        let with_images = book(
            "urn:gutenberg:2701:3",
            vec![format(
                "EPUB3 (E-readers incl. Send-to-Kindle)",
                "/2701.epub3.images",
            )],
        );
        let merged = merge_gutenberg_editions(vec![no_images, with_images]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "urn:gutenberg:2701");
        let labels: Vec<&str> = merged[0].formats.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "EPUB (no images, older E-readers)",
                "EPUB3 (E-readers incl. Send-to-Kindle)"
            ]
        );
    }

    #[test]
    fn merge_keeps_distinct_gutenberg_books_separate() {
        let a = book("urn:gutenberg:2701:2", vec![format("EPUB", "/2701.epub")]);
        let b = book("urn:gutenberg:2489:2", vec![format("EPUB", "/2489.epub")]);
        let merged = merge_gutenberg_editions(vec![a, b]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_leaves_non_gutenberg_ids_untouched() {
        let a = book(
            "https://standardebooks.org/ebooks/herman-melville/moby-dick",
            vec![format("EPUB", "/moby.epub")],
        );
        let b = book(
            "https://standardebooks.org/ebooks/herman-melville/moby-dick",
            vec![format("EPUB", "/moby.epub")],
        );
        let merged = merge_gutenberg_editions(vec![a, b]);
        assert_eq!(merged.len(), 2, "non-Gutenberg ids must pass through");
    }

    #[test]
    fn merge_fills_missing_metadata_from_later_variant() {
        let first = book("urn:gutenberg:2701:2", vec![format("EPUB", "/a.epub")]);
        let mut second = book("urn:gutenberg:2701:3", vec![format("EPUB3", "/b.epub")]);
        second.cover_url = Some("/cover.jpg".into());
        second.language = Some("en".into());
        let merged = merge_gutenberg_editions(vec![first, second]);
        assert_eq!(merged[0].cover_url.as_deref(), Some("/cover.jpg"));
        assert_eq!(merged[0].language.as_deref(), Some("en"));
    }

    #[test]
    fn merge_dedupes_formats_by_href() {
        let a = book("urn:gutenberg:2701:2", vec![format("EPUB", "/same.epub")]);
        let b = book("urn:gutenberg:2701:3", vec![format("EPUB", "/same.epub")]);
        let merged = merge_gutenberg_editions(vec![a, b]);
        assert_eq!(merged[0].formats.len(), 1);
    }

    #[test]
    fn merge_strips_edition_note_from_summary_html() {
        let mut no_images = book("urn:gutenberg:2701:2", vec![format("EPUB", "/a.epub")]);
        no_images.summary_html = Some(
            "<div><p>This edition had all images removed.</p><p>A whale tale.</p></div>".into(),
        );
        let mut with_images = book("urn:gutenberg:2701:3", vec![format("EPUB3", "/b.epub")]);
        with_images.summary_html =
            Some("<div><p>This edition has images.</p><p>A whale tale.</p></div>".into());
        let merged = merge_gutenberg_editions(vec![no_images, with_images]);
        assert_eq!(
            merged[0].summary_html.as_deref(),
            Some("<div><p>A whale tale.</p></div>")
        );
    }

    #[test]
    fn merge_strips_edition_note_from_plain_summary() {
        let mut variant = book("urn:gutenberg:2701:2", vec![format("EPUB", "/a.epub")]);
        variant.summary = Some("This edition had all images removed. A whale tale.".into());
        let merged = merge_gutenberg_editions(vec![variant]);
        assert_eq!(merged[0].summary.as_deref(), Some("A whale tale."));
    }

    #[test]
    fn merge_drops_summary_that_is_only_an_edition_note() {
        let mut variant = book("urn:gutenberg:2701:2", vec![format("EPUB", "/a.epub")]);
        variant.summary = Some("This edition has images.".into());
        variant.summary_html = Some("<div><p>This edition has images.</p></div>".into());
        let merged = merge_gutenberg_editions(vec![variant]);
        assert_eq!(merged[0].summary, None);
        assert_eq!(
            merged[0].summary_html, None,
            "html reduced to empty markup should be dropped"
        );
    }

    #[test]
    fn merge_leaves_non_gutenberg_summaries_untouched() {
        let mut other = book(
            "https://standardebooks.org/ebooks/x",
            vec![format("EPUB", "/x.epub")],
        );
        other.summary = Some("This edition has images. But a real description.".into());
        let merged = merge_gutenberg_editions(vec![other]);
        assert_eq!(
            merged[0].summary.as_deref(),
            Some("This edition has images. But a real description.")
        );
    }

    #[test]
    fn gutenberg_base_id_parses_variant_ids_only() {
        assert_eq!(
            gutenberg_base_id("urn:gutenberg:2701:3").as_deref(),
            Some("urn:gutenberg:2701")
        );
        assert_eq!(gutenberg_base_id("urn:gutenberg:2701"), None);
        assert_eq!(gutenberg_base_id("urn:gutenberg:2701:3:4"), None);
        assert_eq!(gutenberg_base_id("urn:gutenberg:abc:3"), None);
        assert_eq!(gutenberg_base_id("urn:isbn:123:4"), None);
    }

    // ── rel="next" pagination ────────────────────────────────────────────────

    #[test]
    fn parse_feed_extracts_feed_level_next_url() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opds="http://opds-spec.org/2010/catalog">
  <link rel="next" href="/feeds/page/2" type="application/atom+xml"/>
  <entry>
    <id>urn:test:1</id>
    <title>Book One</title>
    <link rel="http://opds-spec.org/acquisition" href="/files/1.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let result = parse_opds_feed_full(xml, "Test").unwrap();
        assert_eq!(result.books.len(), 1);
        assert_eq!(result.next_url.as_deref(), Some("/feeds/page/2"));
    }

    #[test]
    fn parse_feed_no_next_url_when_absent() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:1</id>
    <title>Book One</title>
    <link rel="http://opds-spec.org/acquisition" href="/files/1.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let result = parse_opds_feed_full(xml, "Test").unwrap();
        assert!(result.next_url.is_none());
    }

    #[test]
    fn parse_feed_entry_level_next_link_is_not_feed_next() {
        // A "next" link inside an <entry> (unusual but possible) should NOT be
        // treated as the feed-level pagination link.
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>urn:test:1</id>
    <title>Book One</title>
    <link rel="next" href="/wrong" type="text/html"/>
    <link rel="http://opds-spec.org/acquisition" href="/files/1.epub" type="application/epub+zip"/>
  </entry>
</feed>"#;
        let result = parse_opds_feed_full(xml, "Test").unwrap();
        assert!(
            result.next_url.is_none(),
            "entry-level 'next' link must not set feed_next_url"
        );
    }

    // ── Catalog / BuiltinCatalogId ───────────────────────────────────────────

    #[test]
    fn catalog_resolve_builtin_uses_current_id_name_and_url() {
        let catalog = Catalog::Builtin(BuiltinCatalog {
            id: BuiltinCatalogId::StandardEbooks,
            enabled: false,
        });

        let resolved = catalog.resolve();

        assert_eq!(resolved.name, "Standard Ebooks");
        assert_eq!(
            resolved.search_url,
            BuiltinCatalogId::StandardEbooks.search_url()
        );
        assert!(!resolved.enabled);
    }

    #[test]
    fn catalog_resolve_configured_passes_through_unchanged() {
        let catalog = Catalog::Configured(ConfiguredCatalog {
            name: "My Library".to_string(),
            search_url: "https://example.com/opds?q={searchTerms}".to_string(),
            enabled: true,
        });

        let resolved = catalog.resolve();

        assert_eq!(resolved.name, "My Library");
        assert_eq!(
            resolved.search_url,
            "https://example.com/opds?q={searchTerms}"
        );
        assert!(resolved.enabled);
    }

    #[test]
    fn resolve_catalogs_maps_each_entry() {
        let catalogs = vec![
            Catalog::Builtin(BuiltinCatalog {
                id: BuiltinCatalogId::ProjectGutenberg,
                enabled: true,
            }),
            Catalog::Configured(ConfiguredCatalog {
                name: "My Library".to_string(),
                search_url: "https://example.com/opds".to_string(),
                enabled: false,
            }),
        ];

        let resolved = resolve_catalogs(&catalogs);

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "Project Gutenberg");
        assert_eq!(resolved[1].name, "My Library");
    }
}
