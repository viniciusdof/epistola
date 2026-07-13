use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;
use epistola_format::ClientSpec;
use epistola_http::{ClientConfig, ProxyConfig};

/// Client-behavior flags shared by `run` and the ad-hoc CLI. A flag
/// overrides the matching collection `[client]` default.
#[derive(Args, Debug, Clone, Default)]
pub struct ClientArgs {
    /// Request timeout in seconds (overrides the collection default)
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Max redirects to follow, 0 disables (overrides the collection default)
    #[arg(long = "max-redirects", value_name = "N")]
    pub max_redirects: Option<usize>,

    /// Proxy URL to use for this request (overrides the collection default)
    #[arg(long)]
    pub proxy: Option<String>,

    /// Disable proxying, even if the collection configures one
    #[arg(long)]
    pub no_proxy: bool,

    /// Skip TLS certificate validation. Dangerous — only use against hosts
    /// you trust (e.g. local dev servers with self-signed certs)
    #[arg(long)]
    pub insecure: bool,

    /// Path to a combined cert+key PEM file for mutual-TLS; relative paths
    /// resolve against the collection root
    #[arg(long = "client-cert", value_name = "PATH")]
    pub client_cert: Option<PathBuf>,
}

impl ClientArgs {
    pub fn resolve(&self, collection: &ClientSpec, base_dir: &Path) -> Result<ClientConfig> {
        let proxy = if self.no_proxy {
            ProxyConfig::Disabled
        } else if let Some(url) = self.proxy.clone().or_else(|| collection.proxy.clone()) {
            ProxyConfig::Custom(url)
        } else {
            ProxyConfig::SystemDefault
        };

        let client_identity_pem = match self
            .client_cert
            .clone()
            .or_else(|| collection.client_cert.clone().map(PathBuf::from))
        {
            Some(path) => {
                let full_path = base_dir.join(&path);
                Some(std::fs::read(&full_path).with_context(|| {
                    format!(
                        "failed to read client certificate '{}'",
                        full_path.display()
                    )
                })?)
            }
            None => None,
        };

        Ok(ClientConfig {
            timeout: self
                .timeout
                .or(collection.timeout_secs)
                .map(Duration::from_secs),
            max_redirects: self.max_redirects.or(collection.max_redirects),
            proxy,
            insecure: self.insecure || collection.insecure,
            client_identity_pem,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_http::ProxyConfig;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn cli_flag_overrides_the_collection_default() {
        let args = ClientArgs {
            timeout: Some(5),
            ..Default::default()
        };
        let collection = ClientSpec {
            timeout_secs: Some(30),
            ..Default::default()
        };

        assert_eq!(
            args.resolve(&collection, Path::new(".")).unwrap().timeout,
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn falls_back_to_the_collection_default_when_no_flag_given() {
        let args = ClientArgs::default();
        let collection = ClientSpec {
            max_redirects: Some(3),
            ..Default::default()
        };

        assert_eq!(
            args.resolve(&collection, Path::new("."))
                .unwrap()
                .max_redirects,
            Some(3)
        );
    }

    #[test]
    fn no_proxy_flag_disables_proxying_even_if_the_collection_configures_one() {
        let args = ClientArgs {
            no_proxy: true,
            ..Default::default()
        };
        let collection = ClientSpec {
            proxy: Some("http://proxy.local:8080".to_string()),
            ..Default::default()
        };

        assert!(matches!(
            args.resolve(&collection, Path::new(".")).unwrap().proxy,
            ProxyConfig::Disabled
        ));
    }

    #[test]
    fn defaults_to_system_proxy_when_nothing_is_configured() {
        let args = ClientArgs::default();
        let collection = ClientSpec::default();

        assert!(matches!(
            args.resolve(&collection, Path::new(".")).unwrap().proxy,
            ProxyConfig::SystemDefault
        ));
    }

    #[test]
    fn insecure_flag_enables_when_the_cli_flag_is_set() {
        let args = ClientArgs {
            insecure: true,
            ..Default::default()
        };
        let collection = ClientSpec::default();

        assert!(args.resolve(&collection, Path::new(".")).unwrap().insecure);
    }

    #[test]
    fn insecure_is_enabled_when_the_collection_sets_it_even_without_the_flag() {
        let args = ClientArgs::default();
        let collection = ClientSpec {
            insecure: true,
            ..Default::default()
        };

        assert!(args.resolve(&collection, Path::new(".")).unwrap().insecure);
    }

    #[test]
    fn client_cert_flag_overrides_the_collection_default() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("flag.pem"), b"flag-pem").unwrap();
        std::fs::write(dir.path().join("collection.pem"), b"collection-pem").unwrap();

        let args = ClientArgs {
            client_cert: Some(PathBuf::from("flag.pem")),
            ..Default::default()
        };
        let collection = ClientSpec {
            client_cert: Some("collection.pem".to_string()),
            ..Default::default()
        };

        let config = args.resolve(&collection, dir.path()).unwrap();
        assert_eq!(config.client_identity_pem, Some(b"flag-pem".to_vec()));
    }

    #[test]
    fn client_cert_falls_back_to_the_collection_value_relative_to_base_dir() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("collection.pem"), b"collection-pem").unwrap();

        let args = ClientArgs::default();
        let collection = ClientSpec {
            client_cert: Some("collection.pem".to_string()),
            ..Default::default()
        };

        let config = args.resolve(&collection, dir.path()).unwrap();
        assert_eq!(config.client_identity_pem, Some(b"collection-pem".to_vec()));
    }

    #[test]
    fn client_cert_errors_when_the_file_is_missing() {
        let dir = tempdir().unwrap();
        let args = ClientArgs {
            client_cert: Some(PathBuf::from("nope.pem")),
            ..Default::default()
        };
        let collection = ClientSpec::default();

        assert!(args.resolve(&collection, dir.path()).is_err());
    }
}
