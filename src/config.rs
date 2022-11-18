use std::convert::Infallible;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Author(pub String);

impl FromStr for Author {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Author(s.into()))
    }
}

#[derive(Debug, Clone)]
pub struct Title(pub String);

impl FromStr for Title {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Title(s.into()))
    }
}
