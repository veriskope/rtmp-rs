use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    SampleError,
    Network(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Error::SampleError => write!(f, "Some error string"),
            Error::Network(err) => write!(f, "Network error: {}", err),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Network(err)
    }
}
