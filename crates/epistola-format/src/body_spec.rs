use std::path::Path;

use epistola_core::{Body, Header, VariableResolver};
use serde::{Deserialize, Serialize};

use crate::error::FormatError;

/// `[request.body]`, same tagged pattern as `AuthSpec`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BodySpec {
    #[default]
    None,
    Text {
        content: String,
    },
    Json {
        content: String,
    },
    Form {
        fields: Vec<FormField>,
    },
    Multipart {
        parts: Vec<MultipartPart>,
    },
}

impl BodySpec {
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, BodySpec::None)
    }

    /// Interpolates and encodes into a `Body`, plus a `Content-Type` header
    /// when the encoding mechanically requires one — only `multipart`,
    /// whose boundary must match what actually encoded the body. Relative
    /// `file` part paths resolve against `base_dir`.
    pub(crate) fn resolve(
        &self,
        resolver: &dyn VariableResolver,
        base_dir: &Path,
    ) -> Result<(Body, Option<Header>), FormatError> {
        match self {
            BodySpec::None => Ok((Body::Empty, None)),
            BodySpec::Text { content } | BodySpec::Json { content } => Ok((
                Body::text(epistola_core::interpolate(content, resolver)?),
                None,
            )),
            BodySpec::Form { fields } => {
                let mut serializer = form_urlencoded::Serializer::new(String::new());
                for field in fields.iter().filter(|f| !f.disabled) {
                    let value = epistola_core::interpolate(&field.value, resolver)?;
                    serializer.append_pair(&field.name, &value);
                }
                Ok((Body::text(serializer.finish()), None))
            }
            BodySpec::Multipart { parts } => {
                let boundary = generate_boundary();
                let body = encode_multipart(parts, resolver, base_dir, &boundary)?;
                let content_type = Header::new(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                );
                Ok((Body::Bytes(body), Some(content_type)))
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

/// One part of a `multipart` body. `File` reads its content from disk at
/// resolve time, relative to the request file's directory (or absolute).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MultipartPart {
    Text {
        name: String,
        value: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        disabled: bool,
    },
    File {
        name: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        disabled: bool,
    },
}

impl MultipartPart {
    fn disabled(&self) -> bool {
        match self {
            MultipartPart::Text { disabled, .. } | MultipartPart::File { disabled, .. } => {
                *disabled
            }
        }
    }
}

/// Random-enough boundary that won't collide with real body content. Uses
/// `RandomState`'s OS-seeded hasher instead of pulling in a `rand`
/// dependency for something that isn't a security boundary. Public so the
/// ad-hoc CLI can encode a `multipart` body the same way a saved request
/// does, without duplicating the encoding logic.
pub fn generate_boundary() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("epistola-boundary-{a:016x}{b:016x}")
}

/// Encodes `parts` as a `multipart/form-data` body. Public for the same
/// reason as [`generate_boundary`].
pub fn encode_multipart(
    parts: &[MultipartPart],
    resolver: &dyn VariableResolver,
    base_dir: &Path,
    boundary: &str,
) -> Result<Vec<u8>, FormatError> {
    let mut body = Vec::new();
    for part in parts.iter().filter(|p| !p.disabled()) {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match part {
            MultipartPart::Text { name, value, .. } => {
                let name = epistola_core::interpolate(name, resolver)?;
                let value = epistola_core::interpolate(value, resolver)?;
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
            }
            MultipartPart::File {
                name,
                path,
                filename,
                content_type,
                ..
            } => {
                let name = epistola_core::interpolate(name, resolver)?;
                let path = epistola_core::interpolate(path, resolver)?;
                let filename = match filename {
                    Some(f) => epistola_core::interpolate(f, resolver)?,
                    None => Path::new(&path)
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone()),
                };

                let full_path = base_dir.join(&path);
                let content = std::fs::read(&full_path).map_err(|source| FormatError::Io {
                    path: full_path,
                    source,
                })?;

                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
                    )
                    .as_bytes(),
                );
                if let Some(content_type) = content_type {
                    let content_type = epistola_core::interpolate(content_type, resolver)?;
                    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
                }
                body.extend_from_slice(b"\r\n");
                body.extend_from_slice(&content);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(body)
}
