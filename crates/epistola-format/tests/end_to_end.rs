#![allow(clippy::unwrap_used)]

use epistola_format::{LoadedCollection, RequestFile};
use tempfile::tempdir;

fn write(dir: &std::path::Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn loads_a_full_collection_and_resolves_a_request_end_to_end() {
    let dir = tempdir().unwrap();
    write(
        dir.path(),
        "epistola.toml",
        "name = \"Demo\"\n\n[variables]\napi_version = \"v2\"\n",
    );
    write(
        dir.path(),
        "environments/dev.toml",
        "[variables]\nbase_url = \"https://dev.test\"\n",
    );
    write(
        dir.path(),
        "users/list-users.req.toml",
        r#"
        [request]
        name = "List users"
        method = "GET"
        url = "{{base_url}}/{{api_version}}/users"

        [request.auth]
        type = "bearer"
        token = "{{auth_token}}"

        [request.variables]
        auth_token = "override-token"
        "#,
    );

    let collection = LoadedCollection::discover_from(&dir.path().join("users")).unwrap();
    let mut resolver = collection.resolver_for_environment(Some("dev")).unwrap();

    let file = RequestFile::load(&dir.path().join("users/list-users.req.toml")).unwrap();
    let unresolved = file.to_unresolved();
    resolver = resolver.layer(unresolved.variables.clone());

    let request = unresolved.resolve(&resolver).unwrap();

    assert_eq!(request.url, "https://dev.test/v2/users");
    assert_eq!(request.headers[0].value, "Bearer override-token");
}
