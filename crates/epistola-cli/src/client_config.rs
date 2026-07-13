use std::time::Duration;

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
}

impl ClientArgs {
    pub fn resolve(&self, collection: &ClientSpec) -> ClientConfig {
        let proxy = if self.no_proxy {
            ProxyConfig::Disabled
        } else if let Some(url) = self.proxy.clone().or_else(|| collection.proxy.clone()) {
            ProxyConfig::Custom(url)
        } else {
            ProxyConfig::SystemDefault
        };

        ClientConfig {
            timeout: self
                .timeout
                .or(collection.timeout_secs)
                .map(Duration::from_secs),
            max_redirects: self.max_redirects.or(collection.max_redirects),
            proxy,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_http::ProxyConfig;

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
            args.resolve(&collection).timeout,
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

        assert_eq!(args.resolve(&collection).max_redirects, Some(3));
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
            args.resolve(&collection).proxy,
            ProxyConfig::Disabled
        ));
    }

    #[test]
    fn defaults_to_system_proxy_when_nothing_is_configured() {
        let args = ClientArgs::default();
        let collection = ClientSpec::default();

        assert!(matches!(
            args.resolve(&collection).proxy,
            ProxyConfig::SystemDefault
        ));
    }
}
