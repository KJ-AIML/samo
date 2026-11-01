use samo_domain::extensions::ExtError;
use samo_domain::store::StoreError;
use samo_ext_protocol::ProtocolError;
use std::fmt;

#[derive(Debug)]
pub enum InstallError {
    Io(String),
    Manifest(ProtocolError),
    Package(ExtError),
    NoHelper(String),
    /// The package doesn't match its signed catalog entry (R11).
    Store(StoreError),
    /// Samo won't install it here, and says why.
    Refused(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Manifest(e) => write!(f, "{e}"),
            Self::Package(e) => write!(f, "{e}"),
            Self::NoHelper(path) => write!(f, "its helper `{path}` isn't in the package"),
            Self::Store(e) => write!(f, "{e}"),
            Self::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl InstallError {
    pub(crate) fn io(what: &str, err: std::io::Error) -> Self {
        Self::Io(format!("{what}: {err}"))
    }
}
