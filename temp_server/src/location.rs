use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct Location(String);

impl Location {
    pub(crate) fn path(&self) -> PathBuf {
        PathBuf::from(format!("{}.csv", self.0))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for Location {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Location {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}
