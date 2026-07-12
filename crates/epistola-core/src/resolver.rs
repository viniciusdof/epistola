use std::collections::BTreeMap;

use crate::traits::VariableResolver;

/// A `VariableResolver` built from ordered layers (lowest precedence
/// first); later layers shadow earlier ones for the same key.
#[derive(Debug, Default, Clone)]
pub struct LayeredVariableResolver {
    layers: Vec<BTreeMap<String, String>>,
}

impl LayeredVariableResolver {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn layer(mut self, vars: BTreeMap<String, String>) -> Self {
        self.layers.push(vars);
        self
    }
}

impl VariableResolver for LayeredVariableResolver {
    fn resolve(&self, name: &str) -> Option<String> {
        self.layers
            .iter()
            .rev()
            .find_map(|layer| layer.get(name).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_resolver_resolves_nothing() {
        assert_eq!(LayeredVariableResolver::new().resolve("x"), None);
    }

    #[test]
    fn single_layer_resolves_its_values() {
        let resolver = LayeredVariableResolver::new().layer(BTreeMap::from([(
            "base_url".to_string(),
            "https://a.test".to_string(),
        )]));
        assert_eq!(
            resolver.resolve("base_url").as_deref(),
            Some("https://a.test")
        );
    }

    #[test]
    fn later_layers_override_earlier_layers_for_the_same_key() {
        let resolver = LayeredVariableResolver::new()
            .layer(BTreeMap::from([("k".to_string(), "global".to_string())]))
            .layer(BTreeMap::from([("k".to_string(), "env".to_string())]));
        assert_eq!(resolver.resolve("k").as_deref(), Some("env"));
    }

    #[test]
    fn resolve_returns_none_for_a_key_present_in_no_layer() {
        let resolver = LayeredVariableResolver::new()
            .layer(BTreeMap::from([("k".to_string(), "v".to_string())]));
        assert_eq!(resolver.resolve("other"), None);
    }
}
