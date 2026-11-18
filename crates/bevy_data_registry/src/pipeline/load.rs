//! Loading the data folder, and knowing when every file in it is settled:
//! loaded, or failed to. On first load, and once loaded, after a file
//! changed, was added or removed.

use bevy::asset::io::AssetReaderError;
use bevy::asset::{AssetLoadError, AssetLoadFailedEvent, AssetPath, LoadState, LoadedFolder};
use bevy::prelude::*;

use super::{DataErrors, DataState};
use crate::DataFile;

/// The data folder, loaded with every file in it.
#[derive(Resource)]
pub(crate) struct DataFolder {
    pub path: AssetPath<'static>,
    handle: Option<Handle<LoadedFolder>>,
}

impl DataFolder {
    pub fn new(path: AssetPath<'static>) -> Self {
        Self { path, handle: None }
    }
}

/// Every data file, by path, as last settled: the order definitions are
/// indexed in, so the first of an id used twice is the same each time.
#[derive(Resource, Default)]
pub(crate) struct DataFiles(pub Vec<(AssetId<DataFile>, AssetPath<'static>)>);

/// Where the pipeline is, between frames.
#[derive(Resource, Default)]
pub(crate) struct Progress {
    /// A file changed since it was last indexed (once loaded).
    dirty: bool,
    /// Every file is settled: indexing, validating and finishing are due.
    pub due: bool,
}

/// Starts loading the data folder.
pub(super) fn load(asset_server: Res<AssetServer>, mut folder: ResMut<DataFolder>) {
    folder.handle = Some(asset_server.load_folder(folder.path.clone()));
}

/// Marks indexing due once every file is settled: on first load, once
/// they're all loaded (or failed to); once loaded, after one changed, was
/// added or removed. Lists the files, and what failed to load.
pub(super) fn settle(
    state: Res<State<DataState>>,
    asset_server: Res<AssetServer>,
    folder: Res<DataFolder>,
    folders: Res<Assets<LoadedFolder>>,
    mut file_events: MessageReader<AssetEvent<DataFile>>,
    mut folder_events: MessageReader<AssetEvent<LoadedFolder>>,
    mut failures: MessageReader<AssetLoadFailedEvent<DataFile>>,
    mut progress: ResMut<Progress>,
    mut files: ResMut<DataFiles>,
    mut errors: ResMut<DataErrors>,
) {
    let Some(handle) = &folder.handle else {
        return;
    };
    // Read every frame, so what loading itself sent isn't taken for a
    // change once loaded. Only the data files count: the loader reads any
    // file of its extensions the app loads, wherever it is. `Added` is
    // left out: Bevy sends it for the first load's files a frame late,
    // once loading is over; a file added later shows as the folder
    // modified.
    let ours = |id: AssetId<DataFile>| files.0.iter().any(|(file, _)| *file == id);
    let files_changed = file_events.read().any(|event| match event {
        AssetEvent::Modified { id } | AssetEvent::Removed { id } => ours(*id),
        _ => false,
    });
    let folder_changed = folder_events
        .read()
        .any(|event| event.is_modified(handle.id()));
    let failed = failures.read().any(|failure| ours(failure.id));
    let changed = files_changed || folder_changed || failed;

    match state.get() {
        DataState::Loading if !progress.due => {}
        DataState::Loaded => {
            progress.dirty |= changed;
            if !progress.dirty {
                return;
            }
        }
        _ => return,
    }

    match asset_server.load_state(handle) {
        LoadState::NotLoaded | LoadState::Loading => return,
        LoadState::Failed(error) => {
            files.0.clear();
            if is_not_found(&error) {
                warn!("data folder {} not found", folder.path);
            } else {
                errors.push(format!(
                    "failed to load data folder {}: {error}",
                    folder.path
                ));
            }
        }
        LoadState::Loaded => {
            let Some(loaded) = folders.get(handle) else {
                return;
            };
            let Some(settled) = list(&asset_server, loaded, &mut errors) else {
                return;
            };
            files.0 = settled;
        }
    }
    progress.dirty = false;
    progress.due = true;
}

/// Every data file of `folder`, by path; `None` while one is still
/// loading. Pushes to `errors` those that failed to.
fn list(
    asset_server: &AssetServer,
    folder: &LoadedFolder,
    errors: &mut DataErrors,
) -> Option<Vec<(AssetId<DataFile>, AssetPath<'static>)>> {
    let mut files = Vec::new();
    let mut failed = Vec::new();
    for handle in &folder.handles {
        let Ok(handle) = handle.clone().try_typed::<DataFile>() else {
            // Another loader claims its extension: as data, it'd be lost.
            if let Some(path) = asset_server.get_path(handle.id()) {
                warn!(
                    "{path} is in the data folder but isn't read as data: another asset loader \
                     claims its extension, or it's no data file",
                );
            }
            continue;
        };
        let Some(path) = asset_server.get_path(&handle) else {
            continue;
        };
        match asset_server.load_state(&handle) {
            LoadState::NotLoaded | LoadState::Loading => return None,
            // Removed: Bevy tries to reload it before the folder, which
            // leaves it out once it's loaded again.
            LoadState::Failed(error) if is_not_found(&error) => continue,
            // A file that fails to load again keeps what it last loaded.
            LoadState::Failed(error) => failed.push(error.to_string()),
            LoadState::Loaded => {}
        }
        files.push((handle.id(), path.into_owned()));
    }
    errors.extend(failed);
    files.sort_by(|(_, a), (_, b)| a.path().cmp(b.path()));
    Some(files)
}

/// Whether a load failed because the file or folder isn't there.
fn is_not_found(error: &AssetLoadError) -> bool {
    matches!(
        error,
        AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_))
    )
}
