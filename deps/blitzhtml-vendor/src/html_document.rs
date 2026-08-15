use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

use crate::DocumentHtmlParser;

use blitz_dom::{BaseDocument, DEFAULT_CSS, Document, DocumentConfig};

pub struct HtmlDocument {
    inner: BaseDocument,
    /// Non-fatal html5ever parse errors collected during `from_html`.
    /// TypePress (via fulgur) reads this to surface TP-1008 warnings instead
    /// of printing raw `ERROR:` lines to stdout.
    pub parse_warnings: RefCell<Vec<Cow<'static, str>>>,
}

// Implement DocumentLike and required traits for HtmlDocument
impl Deref for HtmlDocument {
    type Target = BaseDocument;
    fn deref(&self) -> &BaseDocument {
        &self.inner
    }
}
impl DerefMut for HtmlDocument {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl From<HtmlDocument> for BaseDocument {
    fn from(doc: HtmlDocument) -> BaseDocument {
        doc.inner
    }
}
impl Document for HtmlDocument {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl HtmlDocument {
    /// Parse HTML (or XHTML) into an [`HtmlDocument`]
    pub fn from_html(html: &str, mut config: DocumentConfig) -> Self {
        if let Some(ss) = &mut config.ua_stylesheets {
            if !ss.iter().any(|s| s == DEFAULT_CSS) {
                ss.push(String::from(DEFAULT_CSS));
            }
        }
        let mut doc = BaseDocument::new(config);
        let mut mutr = doc.mutate();
        let warnings = DocumentHtmlParser::parse_into_mutator(&mut mutr, html);
        drop(mutr);
        HtmlDocument {
            inner: doc,
            parse_warnings: RefCell::new(warnings),
        }
    }

    /// Convert the [`HtmlDocument`] into it's inner [`BaseDocument`]
    pub fn into_inner(self) -> BaseDocument {
        self.into()
    }
}
