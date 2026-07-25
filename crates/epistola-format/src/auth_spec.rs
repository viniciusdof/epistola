use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use epistola_core::{Header, VariableResolver};
use serde::{Deserialize, Serialize};

use crate::error::FormatError;

/// `[request.auth]`, tagged on `type`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthSpec {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        location: ApiKeyLocation,
        name: String,
        value: String,
    },
}

/// Where an `apikey` auth's name/value pair is placed on the request.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

/// Query params to merge into a request's query string.
type QueryParams = Vec<(String, String)>;

impl AuthSpec {
    /// Interpolates and folds this auth into headers/query params to merge
    /// into a request. Empty for `AuthSpec::None`.
    pub(crate) fn resolve_headers_and_query(
        &self,
        resolver: &dyn VariableResolver,
    ) -> Result<(Vec<Header>, QueryParams), FormatError> {
        let mut headers = Vec::new();
        let mut query = Vec::new();
        match self {
            AuthSpec::None => {}
            AuthSpec::Bearer { token } => {
                let token = epistola_core::interpolate(token, resolver)?;
                headers.push(Header::new("Authorization", format!("Bearer {token}")));
            }
            AuthSpec::Basic { username, password } => {
                let username = epistola_core::interpolate(username, resolver)?;
                let password = epistola_core::interpolate(password, resolver)?;
                let credentials = BASE64.encode(format!("{username}:{password}"));
                headers.push(Header::new("Authorization", format!("Basic {credentials}")));
            }
            AuthSpec::ApiKey {
                location,
                name,
                value,
            } => {
                let name = epistola_core::interpolate(name, resolver)?;
                let value = epistola_core::interpolate(value, resolver)?;
                match location {
                    ApiKeyLocation::Header => headers.push(Header::new(name, value)),
                    ApiKeyLocation::Query => query.push((name, value)),
                }
            }
        }
        Ok((headers, query))
    }
}
