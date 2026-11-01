use samo_domain::extensions::ExtError;
use samo_ext_protocol::ProtocolError;
use std::fmt;

#[derive(Debug)]
pub enum InstallError {
    Io(String),
    Manifest(ProtocolError),
    Package(ExtError),
    NoHelper(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Manifest(e) => write!(f, "{e}"),
            Self::Package(e) => write!(f, "{e}"),
            Self::NoHelper(path) => write!(f, "its helper `{path}` isn't in the package"),
        }
    }
}

impl InstallError {
    pub(crate) fn io(what: &str, err: std::io::Error) -> Self {
        Self::Io(format!("{what}: {err}"))
    }
}
