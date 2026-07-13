use std::path::PathBuf;

use clap::Args;
use epistola_engine::client::ClientOverrides;

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
    pub fn to_overrides(&self) -> ClientOverrides {
        ClientOverrides {
            timeout: self.timeout,
            max_redirects: self.max_redirects,
            proxy: self.proxy.clone(),
            no_proxy: self.no_proxy,
            insecure: self.insecure,
            client_cert: self.client_cert.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn to_overrides_maps_every_field() {
        let args = ClientArgs {
            timeout: Some(5),
            max_redirects: Some(2),
            proxy: Some("http://proxy.local".to_string()),
            no_proxy: true,
            insecure: true,
            client_cert: Some(PathBuf::from("client.pem")),
        };
        let overrides = args.to_overrides();
        assert_eq!(overrides.timeout, Some(5));
        assert_eq!(overrides.max_redirects, Some(2));
        assert_eq!(overrides.proxy.as_deref(), Some("http://proxy.local"));
        assert!(overrides.no_proxy);
        assert!(overrides.insecure);
        assert_eq!(overrides.client_cert, Some(PathBuf::from("client.pem")));
    }

    #[test]
    fn to_overrides_on_the_default_args_is_all_none() {
        let overrides = ClientArgs::default().to_overrides();
        assert_eq!(overrides.timeout, None);
        assert_eq!(overrides.max_redirects, None);
        assert_eq!(overrides.proxy, None);
        assert!(!overrides.no_proxy);
        assert!(!overrides.insecure);
        assert_eq!(overrides.client_cert, None);
    }
}
