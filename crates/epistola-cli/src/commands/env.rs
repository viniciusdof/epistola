use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use epistola_format::{
    create_environment, delete_environment, load_environment, rename_environment,
    set_environment_variable, LoadedCollection,
};

#[derive(Args, Debug)]
pub struct EnvArgs {
    #[command(subcommand)]
    pub action: EnvAction,
}

#[derive(Subcommand, Debug)]
pub enum EnvAction {
    /// Create a new (empty) environment
    New(NewArgs),
    /// Set (or update) a variable in an environment
    Set(SetArgs),
    /// List environments in the current collection
    List(ListArgs),
    /// Delete an environment
    Delete(DeleteArgs),
    /// Rename an environment
    Rename(RenameArgs),
    /// Show or set the environment `run` falls back to when `--env` is omitted
    Default(DefaultArgs),
}

#[derive(Args, Debug)]
pub struct NewArgs {
    pub name: String,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    pub name: String,
    /// KEY=VALUE
    pub assignment: String,
    /// Write to the gitignored `<name>.secrets.toml` sidecar instead of the public file
    #[arg(long)]
    pub secret: bool,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    pub name: String,
}

#[derive(Args, Debug)]
pub struct RenameArgs {
    pub name: String,
    pub new_name: String,
}

#[derive(Args, Debug)]
pub struct DefaultArgs {
    /// Environment to set as default; omit to print the current default
    pub name: Option<String>,
}

pub fn run(args: EnvArgs, cwd: &Path) -> Result<()> {
    match args.action {
        EnvAction::New(a) => new(a, cwd),
        EnvAction::Set(a) => set(a, cwd),
        EnvAction::List(a) => list(a, cwd),
        EnvAction::Delete(a) => delete(a, cwd),
        EnvAction::Rename(a) => rename(a, cwd),
        EnvAction::Default(a) => default(a, cwd),
    }
}

fn collection_root(cwd: &Path) -> Result<PathBuf> {
    Ok(current_collection(cwd)?.root)
}

fn current_collection(cwd: &Path) -> Result<LoadedCollection> {
    LoadedCollection::discover_from(cwd)
        .context("not inside a collection (no epistola.toml found in this or any parent directory)")
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join("epistola.toml")
}

/// Clears `default_environment` in the manifest if it currently points at
/// `name`, so deleting/renaming an environment never leaves a dangling
/// default behind.
fn clear_default_if(collection: &LoadedCollection, name: &str) -> Result<()> {
    if collection.manifest.default_environment.as_deref() != Some(name) {
        return Ok(());
    }
    let mut manifest = collection.manifest.clone();
    manifest.default_environment = None;
    manifest
        .save(&manifest_path(&collection.root))
        .context("failed to update the collection manifest")
}

fn new(args: NewArgs, cwd: &Path) -> Result<()> {
    let root = collection_root(cwd)?;
    let path = create_environment(&root, &args.name)
        .with_context(|| format!("failed to create environment '{}'", args.name))?;
    println!("Created {}", path.display());
    Ok(())
}

fn set(args: SetArgs, cwd: &Path) -> Result<()> {
    let (key, value) = args.assignment.split_once('=').ok_or_else(|| {
        anyhow!(
            "invalid assignment '{}', expected KEY=VALUE",
            args.assignment
        )
    })?;

    let root = collection_root(cwd)?;
    let path = set_environment_variable(&root, &args.name, key, value, args.secret)
        .with_context(|| format!("failed to set '{key}' in environment '{}'", args.name))?;

    println!("Set {key} in {}", path.display());
    Ok(())
}

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let root = collection_root(cwd)?;
    let environments_dir = root.join("environments");

    let mut names = BTreeSet::new();
    if environments_dir.is_dir() {
        for entry in std::fs::read_dir(&environments_dir)
            .with_context(|| format!("failed to read '{}'", environments_dir.display()))?
        {
            let file_name = entry?.file_name();
            let file_name = file_name.to_string_lossy();
            // Longest suffix first, so dev.toml/dev.secrets.toml dedupe to "dev".
            let base = file_name
                .strip_suffix(".secrets.toml")
                .or_else(|| file_name.strip_suffix(".toml"));
            if let Some(base) = base {
                names.insert(base.to_string());
            }
        }
    }

    if args.json {
        let json: Vec<_> = names
            .iter()
            .map(|n| serde_json::json!({ "name": n }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if names.is_empty() {
        println!("No environments found in this collection.");
    } else {
        for name in &names {
            println!("{name}");
        }
    }

    Ok(())
}

fn delete(args: DeleteArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    delete_environment(&collection.root, &args.name)
        .with_context(|| format!("failed to delete environment '{}'", args.name))?;
    clear_default_if(&collection, &args.name)?;
    println!("Deleted environment '{}'", args.name);
    Ok(())
}

fn rename(args: RenameArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;
    rename_environment(&collection.root, &args.name, &args.new_name).with_context(|| {
        format!(
            "failed to rename environment '{}' to '{}'",
            args.name, args.new_name
        )
    })?;

    if collection.manifest.default_environment.as_deref() == Some(args.name.as_str()) {
        let mut manifest = collection.manifest.clone();
        manifest.default_environment = Some(args.new_name.clone());
        manifest
            .save(&manifest_path(&collection.root))
            .context("failed to update the collection manifest")?;
    }

    println!("Renamed environment '{}' to '{}'", args.name, args.new_name);
    Ok(())
}

fn default(args: DefaultArgs, cwd: &Path) -> Result<()> {
    let collection = current_collection(cwd)?;

    let Some(name) = args.name else {
        match &collection.manifest.default_environment {
            Some(name) => println!("{name}"),
            None => println!("No default environment set."),
        }
        return Ok(());
    };

    load_environment(&collection.root, &name)
        .with_context(|| format!("failed to set default environment to '{name}'"))?;

    let mut manifest = collection.manifest.clone();
    manifest.default_environment = Some(name.clone());
    manifest
        .save(&manifest_path(&collection.root))
        .context("failed to update the collection manifest")?;

    println!("Default environment set to '{name}'");
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn set_rejects_an_assignment_without_an_equals_sign() {
        let args = SetArgs {
            name: "dev".to_string(),
            assignment: "no-equals".to_string(),
            secret: false,
        };
        let err = set(args, Path::new(".")).unwrap_err();
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn new_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(new(
            NewArgs {
                name: "dev".to_string()
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn new_creates_an_empty_environment() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();

        new(
            NewArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        assert!(dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn set_creates_the_public_file_by_default() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();

        let args = SetArgs {
            name: "dev".to_string(),
            assignment: "k=v".to_string(),
            secret: false,
        };
        set(args, dir.path()).unwrap();

        assert!(dir.path().join("environments/dev.toml").is_file());
        assert!(!dir.path().join("environments/dev.secrets.toml").is_file());
    }

    #[test]
    fn set_with_secret_writes_the_sidecar() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();

        let args = SetArgs {
            name: "dev".to_string(),
            assignment: "k=v".to_string(),
            secret: true,
        };
        set(args, dir.path()).unwrap();

        assert!(dir.path().join("environments/dev.secrets.toml").is_file());
        assert!(!dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn list_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(list(ListArgs { json: false }, dir.path()).is_err());
    }

    #[test]
    fn list_deduplicates_public_and_secrets_files_for_the_same_environment() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        set(
            SetArgs {
                name: "dev".to_string(),
                assignment: "k=v".to_string(),
                secret: false,
            },
            dir.path(),
        )
        .unwrap();
        set(
            SetArgs {
                name: "dev".to_string(),
                assignment: "k=v".to_string(),
                secret: true,
            },
            dir.path(),
        )
        .unwrap();
        set(
            SetArgs {
                name: "prod".to_string(),
                assignment: "k=v".to_string(),
                secret: false,
            },
            dir.path(),
        )
        .unwrap();

        list(ListArgs { json: true }, dir.path()).unwrap();
    }

    #[test]
    fn list_works_when_no_environments_directory_exists_yet() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        list(ListArgs { json: false }, dir.path()).unwrap();
    }

    #[test]
    fn delete_removes_the_environment() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        new(
            NewArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        delete(
            DeleteArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        assert!(!dir.path().join("environments/dev.toml").is_file());
    }

    #[test]
    fn delete_errors_when_the_environment_is_missing() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        assert!(delete(
            DeleteArgs {
                name: "dev".to_string()
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn delete_clears_the_default_environment_if_it_matches() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        new(
            NewArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();
        default(
            DefaultArgs {
                name: Some("dev".to_string()),
            },
            dir.path(),
        )
        .unwrap();

        delete(
            DeleteArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        let manifest =
            epistola_format::CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment, None);
    }

    #[test]
    fn rename_moves_the_environment_and_preserves_variables() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        set(
            SetArgs {
                name: "dev".to_string(),
                assignment: "k=v".to_string(),
                secret: false,
            },
            dir.path(),
        )
        .unwrap();

        rename(
            RenameArgs {
                name: "dev".to_string(),
                new_name: "staging".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        assert!(!dir.path().join("environments/dev.toml").is_file());
        assert!(dir.path().join("environments/staging.toml").is_file());
    }

    #[test]
    fn rename_updates_the_default_environment_if_it_matches() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        new(
            NewArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();
        default(
            DefaultArgs {
                name: Some("dev".to_string()),
            },
            dir.path(),
        )
        .unwrap();

        rename(
            RenameArgs {
                name: "dev".to_string(),
                new_name: "staging".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        let manifest =
            epistola_format::CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment.as_deref(), Some("staging"));
    }

    #[test]
    fn default_prints_none_when_unset() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        default(DefaultArgs { name: None }, dir.path()).unwrap();
    }

    #[test]
    fn default_errors_when_the_named_environment_does_not_exist() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        assert!(default(
            DefaultArgs {
                name: Some("dev".to_string())
            },
            dir.path()
        )
        .is_err());
    }

    #[test]
    fn default_sets_and_persists_the_default_environment() {
        let dir = tempdir().unwrap();
        epistola_format::CollectionManifest::create(&dir.path().join("epistola.toml"), "n", None)
            .unwrap();
        new(
            NewArgs {
                name: "dev".to_string(),
            },
            dir.path(),
        )
        .unwrap();

        default(
            DefaultArgs {
                name: Some("dev".to_string()),
            },
            dir.path(),
        )
        .unwrap();

        let manifest =
            epistola_format::CollectionManifest::load(&dir.path().join("epistola.toml")).unwrap();
        assert_eq!(manifest.default_environment.as_deref(), Some("dev"));
    }
}
