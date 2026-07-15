use epistola_format::{FolderManifest, RequestFile};

use crate::state::{ActiveFile, AppState};

pub fn validate_and_save(state: &mut AppState, file: &ActiveFile) {
    let Some(text) = state.editor_buffers.get(file).map(|b| b.text.clone()) else {
        return;
    };

    let validation: Result<(), String> = match file {
        ActiveFile::Request(_) => RequestFile::from_toml_str(&text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ActiveFile::Folder(_) => FolderManifest::from_toml_str(&text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        ActiveFile::Environment(_) => {
            epistola_format::validate_variables_toml(&text).map_err(|e| e.to_string())
        }
        ActiveFile::Config | ActiveFile::None => Err("this tab can't be saved".to_string()),
    };

    if let Err(message) = validation {
        if let Some(buffer) = state.editor_buffers.get_mut(file) {
            buffer.save_error = Some(message);
        }
        return;
    }

    let path = match file {
        ActiveFile::Request(path) => path.clone(),
        ActiveFile::Folder(dir) => dir.join("folder.toml"),
        ActiveFile::Environment(name) => {
            let Ok(collection) = state.collection.as_ref() else {
                return;
            };
            collection
                .root
                .join("environments")
                .join(format!("{name}.toml"))
        }
        ActiveFile::Config | ActiveFile::None => return,
    };

    let result = std::fs::write(&path, &text);
    let Some(buffer) = state.editor_buffers.get_mut(file) else {
        return;
    };
    match result {
        Ok(()) => buffer.mark_saved(),
        Err(err) => buffer.save_error = Some(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::HashMap;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::buffer::EditorBuffer;
    use crate::collection::CollectionTree;
    use crate::state::{ResponseSubTab, View};

    fn empty_state() -> AppState {
        AppState {
            cwd: PathBuf::new(),
            view: View::Workspace,
            active_file: ActiveFile::None,
            open_tabs: Vec::new(),
            environment: None,
            collection: Err(String::new()),
            overlay: None,
            activity: HashMap::new(),
            response_subtab: ResponseSubTab::default(),
            collection_action_error: None,
            recent_collections: Vec::new(),
            editor_buffers: HashMap::new(),
            overlay_query: String::new(),
            overlay_selected: 0,
            overlay_error: None,
        }
    }

    fn state_with_buffer(file: ActiveFile, text: &str) -> AppState {
        let mut state = empty_state();
        state
            .editor_buffers
            .insert(file, EditorBuffer::new(text.to_string()));
        state
    }

    #[test]
    fn valid_request_toml_is_written_verbatim_and_clears_dirty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        let text =
            "# a comment\n[request]\nname = \"A\"\nmethod = \"GET\"\nurl = \"https://x.test\"\n";
        std::fs::write(&path, "placeholder").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut state = state_with_buffer(file.clone(), text);

        validate_and_save(&mut state, &file);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(!state.editor_buffers[&file].is_dirty());
        assert!(state.editor_buffers[&file].save_error.is_none());
    }

    #[test]
    fn invalid_request_toml_is_not_written_and_sets_save_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.req.toml");
        std::fs::write(&path, "original").unwrap();

        let file = ActiveFile::Request(path.clone());
        let mut state = state_with_buffer(file.clone(), "name = \"A\"\n");
        state
            .editor_buffers
            .get_mut(&file)
            .unwrap()
            .replace_range(0..0, "not valid [ toml\n");

        validate_and_save(&mut state, &file);

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert!(state.editor_buffers[&file].is_dirty());
        assert!(state.editor_buffers[&file].save_error.is_some());
    }

    #[test]
    fn valid_folder_toml_is_saved_to_folder_toml_in_the_given_dir() {
        let dir = tempdir().unwrap();
        let file = ActiveFile::Folder(dir.path().to_path_buf());
        let text = "[[headers]]\nname = \"X\"\nvalue = \"1\"\n";
        let mut state = state_with_buffer(file.clone(), text);

        validate_and_save(&mut state, &file);

        assert_eq!(
            std::fs::read_to_string(dir.path().join("folder.toml")).unwrap(),
            text
        );
        assert!(!state.editor_buffers[&file].is_dirty());
    }

    #[test]
    fn valid_environment_toml_is_saved_under_the_collection_environments_dir() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("environments")).unwrap();

        let file = ActiveFile::Environment("dev".to_string());
        let text = "[variables]\nbase_url = \"https://dev.test\"\n";
        let mut state = state_with_buffer(file.clone(), text);
        state.collection = Ok(CollectionTree {
            root: dir.path().to_path_buf(),
            name: "test".to_string(),
            folders: Vec::new(),
            requests: Vec::new(),
            environments: vec!["dev".to_string()],
            default_environment: None,
            index: Default::default(),
        });

        validate_and_save(&mut state, &file);

        assert_eq!(
            std::fs::read_to_string(dir.path().join("environments").join("dev.toml")).unwrap(),
            text
        );
        assert!(!state.editor_buffers[&file].is_dirty());
    }
}
