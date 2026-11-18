//! Loading the data folder, and knowing when every file in it is settled:
//! loaded, or failed to. On first load, and once loaded, after a file
//! changed, was added or removed.

use std::path::Path;

use bevy::asset::io::AssetReaderError;
use bevy::asset::{AssetLoadError, AssetLoadFailedEvent, AssetPath, LoadState, LoadedFolder};
use bevy::prelude::*;

use super::{DataErrors, DataState};
use crate::file::{DataFile, in_data};

/// The data folder, loaded with every file in it.
#[derive(Resource)]
pub(crate) struct DataFolder {
    pub path: AssetPath<'static>,
    handle: Option<Handle<LoadedFolder>>,
    /// Files that failed to load as they were added, once loaded: held, so
    /// Bevy reloads each when it's saved again. The folder's reload failed
    /// with them, and is asked for again once one loads.
    broken: Vec<(AssetPath<'static>, Handle<DataFile>)>,
    /// Whether it's been listed again on first load, as a file went
    /// missing: only once, so one that always is fails loading.
    relisted: bool,
}

impl DataFolder {
    pub fn new(path: AssetPath<'static>) -> Self {
        Self {
            path,
            handle: None,
            broken: Vec::new(),
            relisted: false,
        }
    }

    /// Whether `path` is in the folder, at any depth: as the loader sees it.
    fn holds(&self, path: &AssetPath) -> bool {
        in_data(&self.path, path).is_some()
    }

    /// Whether `missing`, as the asset reader names it, is the folder
    /// itself rather than a file in it. The folder at the source's root has
    /// no name to tell it by: what's missing is then taken for a file.
    fn is_itself(&self, missing: &Path) -> bool {
        let own = self.path.path();
        !own.as_os_str().is_empty() && missing.ends_with(own)
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

/// Starts loading the data folder; fails loading if its asset source isn't
/// registered, as Bevy only logs that, leaving the folder loading forever.
pub(super) fn load(
    asset_server: Res<AssetServer>,
    mut folder: ResMut<DataFolder>,
    mut progress: ResMut<Progress>,
    mut errors: ResMut<DataErrors>,
) {
    let source = folder.path.source();
    if asset_server.get_source(source).is_err() {
        errors.push(format!(
            "data folder {}: asset source {source} isn't registered",
            folder.path
        ));
        progress.due = true;
        return;
    }
    folder.handle = Some(asset_server.load_folder(folder.path.clone()));
}

/// Marks indexing due once every file is settled: on first load, once
/// they're all loaded (or failed to); once loaded, after one changed, was
/// added or removed. Lists the files, and what failed to load.
pub(super) fn settle(
    state: Res<State<DataState>>,
    asset_server: Res<AssetServer>,
    mut folder: ResMut<DataFolder>,
    folders: Res<Assets<LoadedFolder>>,
    mut file_events: MessageReader<AssetEvent<DataFile>>,
    mut folder_events: MessageReader<AssetEvent<LoadedFolder>>,
    mut failures: MessageReader<AssetLoadFailedEvent<DataFile>>,
    mut folder_failures: MessageReader<AssetLoadFailedEvent<LoadedFolder>>,
    mut progress: ResMut<Progress>,
    mut files: ResMut<DataFiles>,
    mut errors: ResMut<DataErrors>,
) {
    let Some(handle) = folder.handle.clone() else {
        return;
    };
    let loaded = *state.get() == DataState::Loaded;
    // Read every frame, so what loading itself sent isn't taken for a
    // change once loaded. Only the data files count: the loader reads any
    // file of its extensions the app loads, wherever it is. `Added` is
    // left out: Bevy sends it for the first load's files a frame late,
    // once loading is over; a file added later shows as the folder
    // modified.
    let ours = |id: AssetId<DataFile>| files.0.iter().any(|(file, _)| *file == id);
    let broken = |folder: &DataFolder, id: AssetId<DataFile>| {
        folder.broken.iter().any(|(_, file)| file.id() == id)
    };
    let mut files_changed = false;
    let mut fixed = false;
    for event in file_events.read() {
        match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } if ours(id) => {
                files_changed = true;
            }
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
                if broken(&folder, id) =>
            {
                fixed = true;
            }
            _ => {}
        }
    }
    let mut failed = false;
    for failure in failures.read() {
        if ours(failure.id) {
            failed = true;
        } else if loaded
            && folder.holds(&failure.path)
            && !folder.broken.iter().any(|(path, _)| *path == failure.path)
        {
            // Added, and failing the folder's reload with it.
            let file = asset_server.load(failure.path.clone());
            folder.broken.push((failure.path.clone(), file));
        }
    }
    // Every message read, not up to the first that counts: one left unread
    // would be taken for another change next frame.
    let folder_changed = folder_events
        .read()
        .fold(false, |any, event| any | event.is_modified(handle.id()));
    let folder_failed = folder_failures
        .read()
        .fold(false, |any, failure| any | (failure.id == handle.id()));
    if folder_changed {
        folder.broken.clear();
    } else if fixed {
        // The folder's reload failed: asking for it again reloads it.
        folder.broken.clear();
        folder.handle = Some(asset_server.load_folder(folder.path.clone()));
    }
    let changed = files_changed || folder_changed || failed || folder_failed;

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

    match asset_server.load_state(&handle) {
        LoadState::NotLoaded | LoadState::Loading => return,
        // Its last listing still holds every file it had: keep them.
        LoadState::Failed(error) if loaded => errors.push(format!(
            "failed to reload data folder {}, kept as it was: {error}",
            folder.path
        )),
        LoadState::Failed(error) => {
            match missing(&error) {
                Some(path) if folder.is_itself(path) => {
                    warn!("data folder {} not found", folder.path);
                }
                // A file deleted between Bevy listing the folder and
                // loading it: listing it again leaves the file out. Once:
                // one listed but never there (a broken link) is an error.
                Some(_) if !folder.relisted => {
                    folder.relisted = true;
                    folder.handle = Some(asset_server.load_folder(folder.path.clone()));
                    return;
                }
                _ => errors.push(format!(
                    "failed to load data folder {}: {error}",
                    folder.path
                )),
            }
            files.0.clear();
        }
        LoadState::Loaded => {
            let Some(listing) = folders.get(&handle) else {
                return;
            };
            let Some(settled) = list(&asset_server, listing, &mut errors) else {
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
    missing(error).is_some()
}

/// What a load found missing, as the asset reader names it: for files, the
/// full path.
fn missing(error: &AssetLoadError) -> Option<&Path> {
    match error {
        AssetLoadError::AssetReaderError(AssetReaderError::NotFound(path)) => Some(path),
        _ => None,
    }
}
