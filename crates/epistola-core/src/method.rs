use std::fmt;
use std::str::FromStr;

/// An HTTP request method. `Other` covers anything non-standard.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Query,
    Other(String),
}

impl Method {
    pub fn as_str(&self) -> &str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
            Method::Query => "QUERY",
            Method::Other(s) => s,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_ascii_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            "QUERY" => Method::Query,
            other => Method::Other(other.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_known_verbs_case_insensitively() {
        assert_eq!("get".parse::<Method>().unwrap(), Method::Get);
        assert_eq!("Post".parse::<Method>().unwrap(), Method::Post);
        assert_eq!("DELETE".parse::<Method>().unwrap(), Method::Delete);
        assert_eq!("query".parse::<Method>().unwrap(), Method::Query);
    }

    #[test]
    fn unknown_verb_becomes_other() {
        let method = "PROPFIND".parse::<Method>().unwrap();
        assert_eq!(method, Method::Other("PROPFIND".to_string()));
        assert_eq!(method.as_str(), "PROPFIND");
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(Method::Get.to_string(), "GET");
        assert_eq!(Method::Other("FOO".to_string()).to_string(), "FOO");
    }

    #[test]
    fn as_str_covers_every_known_verb() {
        let cases = [
            (Method::Get, "GET"),
            (Method::Post, "POST"),
            (Method::Put, "PUT"),
            (Method::Patch, "PATCH"),
            (Method::Delete, "DELETE"),
            (Method::Head, "HEAD"),
            (Method::Options, "OPTIONS"),
            (Method::Query, "QUERY"),
        ];
        for (method, expected) in cases {
            assert_eq!(method.as_str(), expected);
        }
    }
}
