use std::path::Path;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use epistola_engine::environments;

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

fn new(args: NewArgs, cwd: &Path) -> Result<()> {
    let path = environments::new_environment(cwd, &args.name)
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

    let path = environments::set_variable(cwd, &args.name, key, value, args.secret)
        .with_context(|| format!("failed to set '{key}' in environment '{}'", args.name))?;

    println!("Set {key} in {}", path.display());
    Ok(())
}

fn list(args: ListArgs, cwd: &Path) -> Result<()> {
    let names = environments::list_environment_names(cwd)?;

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
    environments::delete(cwd, &args.name)
        .with_context(|| format!("failed to delete environment '{}'", args.name))?;
    println!("Deleted environment '{}'", args.name);
    Ok(())
}

fn rename(args: RenameArgs, cwd: &Path) -> Result<()> {
    environments::rename(cwd, &args.name, &args.new_name).with_context(|| {
        format!(
            "failed to rename environment '{}' to '{}'",
            args.name, args.new_name
        )
    })?;
    println!("Renamed environment '{}' to '{}'", args.name, args.new_name);
    Ok(())
}

fn default(args: DefaultArgs, cwd: &Path) -> Result<()> {
    let Some(name) = args.name else {
        match environments::get_default(cwd)? {
            Some(name) => println!("{name}"),
            None => println!("No default environment set."),
        }
        return Ok(());
    };

    environments::set_default(cwd, &name)
        .with_context(|| format!("failed to set default environment to '{name}'"))?;

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
    fn list_errors_outside_a_collection() {
        let dir = tempdir().unwrap();
        assert!(list(ListArgs { json: false }, dir.path()).is_err());
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
    fn rename_moves_the_environment() {
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
