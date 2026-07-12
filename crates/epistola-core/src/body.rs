/// A request or response body. Raw bytes only — encoding (JSON, form) is
/// the caller's concern.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Body {
    #[default]
    Empty,
    Bytes(Vec<u8>),
}

impl Body {
    pub fn text(text: impl Into<String>) -> Self {
        Body::Bytes(text.into().into_bytes())
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Body::Empty) || matches!(self, Body::Bytes(b) if b.is_empty())
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Body::Empty => &[],
            Body::Bytes(b) => b,
        }
    }

    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_variant_is_empty() {
        assert!(Body::Empty.is_empty());
        assert_eq!(Body::Empty.len(), 0);
    }

    #[test]
    fn empty_byte_vec_is_also_empty() {
        assert!(Body::Bytes(Vec::new()).is_empty());
    }

    #[test]
    fn text_roundtrips_through_bytes() {
        let body = Body::text("hello");
        assert!(!body.is_empty());
        assert_eq!(body.len(), 5);
        assert_eq!(body.as_bytes(), b"hello");
    }
}
