use std::fmt;
use std::str::FromStr;
#[derive(Clone, Debug, PartialEq)]
pub enum RecordFlag {
    Live,
    Record,
    Append,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Error {
    RecordFlagParse,
}

impl FromStr for RecordFlag {
    type Err = Error;

    fn from_str(from: &str) -> Result<Self, Self::Err> {
        let s = from.to_lowercase();
        match s.as_str() {
            "live" => Ok(RecordFlag::Live),
            "record" => Ok(RecordFlag::Record),
            "append" => Ok(RecordFlag::Append),
            _ => Err(Error::RecordFlagParse),
        }
    }
}

impl fmt::Display for RecordFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordFlag::Live => write!(f, "live"),
            RecordFlag::Record => write!(f, "record"),
            RecordFlag::Append => write!(f, "append"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*; // importing names from outer (for mod tests) scope.

    #[test]
    fn live_from_str() {
        assert_eq!(RecordFlag::from_str("live"), Ok(RecordFlag::Live));
    }
}
