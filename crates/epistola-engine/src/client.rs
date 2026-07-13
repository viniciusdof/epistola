use std::path::{Path, PathBuf};
use std::time::Duration;

use epistola_format::ClientSpec;
use epistola_http::{ClientConfig, ProxyConfig};

use crate::error::EngineError;

/// Plain-data equivalent of `epistola-cli`'s `ClientArgs`, so this merge
/// logic isn't tied to a clap struct.
#[derive(Debug, Clone, Default)]
pub struct ClientOverrides {
    pub timeout: Option<u64>,
    pub max_redirects: Option<usize>,
    pub proxy: Option<String>,
    pub no_proxy: bool,
    pub insecure: bool,
    pub client_cert: Option<PathBuf>,
}

/// Merges CLI/GUI-supplied overrides with a collection's `[client]`
/// defaults — an override always wins.
pub fn resolve_client_config(
    overrides: &ClientOverrides,
    collection: &ClientSpec,
    base_dir: &Path,
) -> Result<ClientConfig, EngineError> {
    let proxy = if overrides.no_proxy {
        ProxyConfig::Disabled
    } else if let Some(url) = overrides.proxy.clone().or_else(|| collection.proxy.clone()) {
        ProxyConfig::Custom(url)
    } else {
        ProxyConfig::SystemDefault
    };

    let client_identity_pem = match overrides
        .client_cert
        .clone()
        .or_else(|| collection.client_cert.clone().map(PathBuf::from))
    {
        Some(path) => {
            let full_path = base_dir.join(&path);
            Some(std::fs::read(&full_path).map_err(|source| EngineError::Io {
                path: full_path,
                source,
            })?)
        }
        None => None,
    };

    Ok(ClientConfig {
        timeout: overrides
            .timeout
            .or(collection.timeout_secs)
            .map(Duration::from_secs),
        max_redirects: overrides.max_redirects.or(collection.max_redirects),
        proxy,
        insecure: overrides.insecure || collection.insecure,
        client_identity_pem,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_http::ProxyConfig;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn cli_flag_overrides_the_collection_default() {
        let overrides = ClientOverrides {
            timeout: Some(5),
            ..Default::default()
        };
        let collection = ClientSpec {
            timeout_secs: Some(30),
            ..Default::default()
        };

        assert_eq!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .timeout,
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn falls_back_to_the_collection_default_when_no_flag_given() {
        let overrides = ClientOverrides::default();
        let collection = ClientSpec {
            max_redirects: Some(3),
            ..Default::default()
        };

        assert_eq!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .max_redirects,
            Some(3)
        );
    }

    #[test]
    fn no_proxy_flag_disables_proxying_even_if_the_collection_configures_one() {
        let overrides = ClientOverrides {
            no_proxy: true,
            ..Default::default()
        };
        let collection = ClientSpec {
            proxy: Some("http://proxy.local:8080".to_string()),
            ..Default::default()
        };

        assert!(matches!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .proxy,
            ProxyConfig::Disabled
        ));
    }

    #[test]
    fn defaults_to_system_proxy_when_nothing_is_configured() {
        let overrides = ClientOverrides::default();
        let collection = ClientSpec::default();

        assert!(matches!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .proxy,
            ProxyConfig::SystemDefault
        ));
    }

    #[test]
    fn insecure_flag_enables_when_the_override_is_set() {
        let overrides = ClientOverrides {
            insecure: true,
            ..Default::default()
        };
        let collection = ClientSpec::default();

        assert!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .insecure
        );
    }

    #[test]
    fn insecure_is_enabled_when_the_collection_sets_it_even_without_the_override() {
        let overrides = ClientOverrides::default();
        let collection = ClientSpec {
            insecure: true,
            ..Default::default()
        };

        assert!(
            resolve_client_config(&overrides, &collection, Path::new("."))
                .unwrap()
                .insecure
        );
    }

    #[test]
    fn client_cert_override_wins_over_the_collection_default() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("flag.pem"), b"flag-pem").unwrap();
        std::fs::write(dir.path().join("collection.pem"), b"collection-pem").unwrap();

        let overrides = ClientOverrides {
            client_cert: Some(PathBuf::from("flag.pem")),
            ..Default::default()
        };
        let collection = ClientSpec {
            client_cert: Some("collection.pem".to_string()),
            ..Default::default()
        };

        let config = resolve_client_config(&overrides, &collection, dir.path()).unwrap();
        assert_eq!(config.client_identity_pem, Some(b"flag-pem".to_vec()));
    }

    #[test]
    fn client_cert_falls_back_to_the_collection_value_relative_to_base_dir() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("collection.pem"), b"collection-pem").unwrap();

        let overrides = ClientOverrides::default();
        let collection = ClientSpec {
            client_cert: Some("collection.pem".to_string()),
            ..Default::default()
        };

        let config = resolve_client_config(&overrides, &collection, dir.path()).unwrap();
        assert_eq!(config.client_identity_pem, Some(b"collection-pem".to_vec()));
    }

    #[test]
    fn client_cert_errors_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        let overrides = ClientOverrides {
            client_cert: Some(PathBuf::from("nope.pem")),
            ..Default::default()
        };
        let collection = ClientSpec::default();

        assert!(resolve_client_config(&overrides, &collection, dir.path()).is_err());
    }
}
