use std::time::Duration;

use crate::request::Header;

/// A backend-agnostic HTTP response, as produced by any `HttpExecutor`.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
    pub duration: Duration,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn body_as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: Vec<u8>) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body,
            duration: Duration::ZERO,
        }
    }

    #[test]
    fn success_range_is_200_to_299() {
        assert!(response(200, vec![]).is_success());
        assert!(response(299, vec![]).is_success());
        assert!(!response(199, vec![]).is_success());
        assert!(!response(300, vec![]).is_success());
        assert!(!response(404, vec![]).is_success());
    }

    #[test]
    fn body_as_str_rejects_invalid_utf8() {
        assert_eq!(response(200, b"hello".to_vec()).body_as_str(), Ok("hello"));
        assert!(response(200, vec![0xff, 0xfe]).body_as_str().is_err());
    }
}
