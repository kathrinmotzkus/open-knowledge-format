use crate::Document;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentQuery {
    Exact(String),
    Partial(String),
}

impl DocumentQuery {
    pub(crate) fn into_parts(self) -> (String, bool) {
        match self {
            Self::Exact(value) => (value, false),
            Self::Partial(value) => (value, true),
        }
    }
}

pub(crate) fn matches(document: &Document, query: &str, partial: bool) -> bool {
    if partial {
        contains(document.relative_path().to_string_lossy().as_ref(), query)
            || contains(document.filename(), query)
            || contains(document.stem(), query)
            || contains(document.title(), query)
            || document.topic().is_some_and(|topic| contains(topic, query))
    } else {
        document
            .relative_path()
            .to_string_lossy()
            .eq_ignore_ascii_case(query)
            || document.filename().eq_ignore_ascii_case(query)
            || document.stem().eq_ignore_ascii_case(query)
            || document.title().eq_ignore_ascii_case(query)
    }
}

fn contains(value: &str, query: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase())
}
