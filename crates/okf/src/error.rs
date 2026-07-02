use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum OkfError {
    NoRoots,
    NoUsableRoots,
    ReadRoot {
        root: PathBuf,
        source: io::Error,
    },
    ReadDocument {
        path: PathBuf,
        source: io::Error,
    },
    InvalidUtf8 {
        path: PathBuf,
    },
    EmptyQuery,
    NotFound {
        query: String,
    },
    Ambiguous {
        query: String,
        matches: Vec<PathBuf>,
    },
}

impl fmt::Display for OkfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRoots => formatter.write_str("no OKF document roots were provided"),
            Self::NoUsableRoots => formatter.write_str("no usable OKF document roots were found"),
            Self::ReadRoot { root, .. } => {
                write!(formatter, "could not read OKF root {}", root.display())
            }
            Self::ReadDocument { path, .. } => {
                write!(formatter, "could not read OKF document {}", path.display())
            }
            Self::InvalidUtf8 { path } => {
                write!(
                    formatter,
                    "OKF document is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::EmptyQuery => formatter.write_str("OKF document query is empty"),
            Self::NotFound { query } => write!(formatter, "OKF document not found: {query}"),
            Self::Ambiguous { query, .. } => {
                write!(formatter, "OKF document query is ambiguous: {query}")
            }
        }
    }
}

impl Error for OkfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReadRoot { source, .. } | Self::ReadDocument { source, .. } => Some(source),
            _ => None,
        }
    }
}
