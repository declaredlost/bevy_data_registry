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
    /// Files that failed the folder's reload as they were added, once
    /// loaded: held, so Bevy reloads each when it's saved again (or its
    /// `.meta`). The folder is asked for again once one loads.
    broken: Vec<(AssetPath<'static>, Handle<DataFile>)>,
    /// Files held as broken that have loaded since, indexed as they are
    /// until a listing has them: one from a scan begun before they failed
    /// can leave them out, and the folder then isn't scanned again.
    recovered: Vec<(AssetPath<'static>, Handle<DataFile>)>,
    /// Whether it's been listed again on first load, as something went
    /// missing: only once, so what always is fails loading.
    relisted: bool,
}

impl DataFolder {
    pub fn new(path: AssetPath<'static>) -> Self {
        Self {
            path,
            handle: None,
            broken: Vec::new(),
            recovered: Vec::new(),
            relisted: false,
        }
    }

    /// Whether `path` is in the folder, at any depth: as the loader sees it.
    fn holds(&self, path: &AssetPath) -> bool {
        in_data(&self.path, path).is_some()
    }

    /// Holds `path`, a file that failed the folder's reload, until it's
    /// fixed: unless it's not in the folder, or already held.
    fn hold(&mut self, asset_server: &AssetServer, path: &AssetPath<'static>) {
        if self.holds(path) && !self.broken.iter().any(|(held, _)| held == path) {
            let file = asset_server.load(path.clone());
            self.broken.push((path.clone(), file));
        }
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
    let failed = failures
        .read()
        .fold(false, |any, failure| any | ours(failure.id));
    // Every message read, not up to the first that counts: one left unread
    // would be taken for another change next frame.
    let folder_changed = folder_events
        .read()
        .fold(false, |any, event| any | event.is_modified(handle.id()));
    let folder_failed = folder_failures
        .read()
        .fold(false, |any, failure| any | (failure.id == handle.id()));
    if loaded
        && folder_failed
        && let LoadState::Failed(error) = asset_server.load_state(&handle)
        && let Some(path) = failing_file(&error)
    {
        // Added, or changed, and failing the folder's reload with it.
        folder.hold(&asset_server, &path);
    }
    if folder_changed {
        // Those the new listing has are fixed; the rest may have failed
        // after it started, so are held still.
        let listed: Vec<AssetPath> = folders
            .get(&handle)
            .into_iter()
            .flat_map(|listing| &listing.handles)
            .filter_map(|file| asset_server.get_path(file.id()))
            .collect();
        folder.broken.retain(|(path, _)| !listed.contains(path));
        folder.recovered.retain(|(path, _)| !listed.contains(path));
    } else if fixed {
        // The folder's reload failed: asking for it again reloads it, if
        // it's failed still; and what loaded is indexed meanwhile.
        let (loaded, broken): (Vec<_>, Vec<_>) = core::mem::take(&mut folder.broken)
            .into_iter()
            .partition(|(_, file)| asset_server.is_loaded(file));
        folder.broken = broken;
        folder.recovered.extend(loaded);
        folder.handle = Some(asset_server.load_folder(folder.path.clone()));
    }
    let changed = files_changed || folder_changed || failed || folder_failed || fixed;

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
            if first_load_failed(&asset_server, &mut folder, &error, &mut errors) {
                return;
            }
            files.0.clear();
        }
        LoadState::Loaded => {
            let Some(listing) = folders.get(&handle) else {
                return;
            };
            let Some(mut settled) = list(&asset_server, listing, &mut errors) else {
                return;
            };
            for (path, file) in &folder.recovered {
                if !settled.iter().any(|(_, listed)| listed == path) {
                    settled.push((file.id(), path.clone()));
                }
            }
            settled.sort_by(|(_, a), (_, b)| a.path().cmp(b.path()));
            files.0 = settled;
        }
    }
    progress.dirty = false;
    progress.due = true;
}

/// Handles the folder failing to load on first load: lists it again (once)
/// if something went missing, and returns whether it did; otherwise says
/// why in `errors`.
fn first_load_failed(
    asset_server: &AssetServer,
    folder: &mut DataFolder,
    error: &AssetLoadError,
    errors: &mut DataErrors,
) -> bool {
    match error {
        // A file deleted between Bevy listing the folder and
        // loading it: listing it again leaves the file out. Once:
        // one listed but never there (a broken link), or the
        // folder itself missing, is an error.
        error if is_not_found(error) && !folder.relisted => {
            folder.relisted = true;
            folder.handle = Some(asset_server.load_folder(folder.path.clone()));
            return true;
        }
        // Bevy's `load_folder` skips a file no loader reads by
        // extension, but not this.
        AssetLoadError::MissingAssetLoader { asset_path, .. } => errors.push(format!(
            "{asset_path} is in data folder {}, but no asset loader reads it: move it \
             out of the folder",
            folder.path
        )),
        error => errors.push(format!(
            "failed to load data folder {}: {error}",
            folder.path
        )),
    }
    false
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

/// The file a folder's load failed on, if the error names one: one its
/// loader failed on, or whose `.meta` is malformed.
fn failing_file(error: &AssetLoadError) -> Option<AssetPath<'static>> {
    match error {
        AssetLoadError::AssetLoaderError(error) => Some(error.path().clone()),
        AssetLoadError::DeserializeMeta { path, .. } => Some(path.clone()),
        _ => None,
    }
}

/// What a load found missing, as the asset reader names it: for files, the
/// full path.
fn missing(error: &AssetLoadError) -> Option<&Path> {
    match error {
        AssetLoadError::AssetReaderError(AssetReaderError::NotFound(path)) => Some(path),
        _ => None,
    }
}
