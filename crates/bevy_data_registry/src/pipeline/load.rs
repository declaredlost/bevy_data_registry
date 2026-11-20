//! Loading the data folder, and knowing when every file in it is settled:
//! loaded, or failed to. On first load, and once loaded, after a file
//! changed, was added or removed.

use std::path::Path;

use bevy::asset::io::{AssetReaderError, AssetSourceId};
use bevy::asset::{
    AssetLoadError, AssetLoadFailedEvent, AssetPath, AssetServerMode, LoadState, LoadedFolder,
};
use bevy::ecs::system::SystemParam;
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

/// The data folder, as systems use it: where it is and what's tracked of
/// it, the asset server loading it, and its listings.
#[derive(SystemParam)]
pub(super) struct Folder<'w> {
    asset_server: Res<'w, AssetServer>,
    data: ResMut<'w, DataFolder>,
    listings: Res<'w, Assets<LoadedFolder>>,
}

impl Folder<'_> {
    /// Asks for the folder: listed and its files loaded, or listed again
    /// if it was before.
    ///
    /// # Errors
    ///
    /// If its asset source isn't registered, or has no processed reader
    /// where assets are processed: Bevy only logs that, leaving the folder
    /// loading forever.
    fn load(&mut self) -> Result<(), String> {
        let id = self.data.path.source();
        let name = match id {
            AssetSourceId::Default => "default",
            AssetSourceId::Name(name) => name,
        };
        let error = |problem: &str| {
            format!(
                "data folder {}: asset source `{name}` {problem}",
                self.data.path
            )
        };
        let Ok(source) = self.asset_server.get_source(id) else {
            return Err(error("isn't registered"));
        };
        if self.asset_server.mode() == AssetServerMode::Processed
            && source.processed_reader().is_err()
        {
            return Err(error("has no processed reader, and assets are processed"));
        }
        self.data.handle = Some(self.asset_server.load_folder(self.data.path.clone()));
        Ok(())
    }

    /// Its handle, once asked for.
    fn handle(&self) -> Option<AssetId<LoadedFolder>> {
        self.data.handle.as_ref().map(Handle::id)
    }

    /// Holds the files its reloads failed on, `failing`: so Bevy reloads
    /// each when it's saved again. Returns whether one has loaded already:
    /// fixed since it failed, with no message to say so.
    fn hold_failing(&mut self, failing: &[AssetPath<'static>]) -> bool {
        let mut loaded = false;
        for path in failing {
            let held = |files: &[(AssetPath, Handle<DataFile>)]| {
                files.iter().any(|(held, _)| held == path)
            };
            if self.data.holds(path) && !held(&self.data.broken) && !held(&self.data.recovered) {
                let file = self.asset_server.load(path.clone());
                loaded |= self.asset_server.is_loaded(&file);
                self.data.broken.push((path.clone(), file));
            }
        }
        loaded
    }

    /// Keeps what's broken and recovered up to date, as `seen`: after a
    /// listing, and once a broken file loads.
    fn track(&mut self, seen: &Seen) {
        // Those found gone, loaded again as a listing left them out (below).
        let asset_server = &self.asset_server;
        self.data.recovered.retain(|(_, file)| {
            !matches!(
                asset_server.load_state(file),
                LoadState::Failed(error) if is_not_found(&error)
            )
        });
        if seen.folder {
            // Those the new listing has are fixed; the rest may have failed
            // after it started, so are held still.
            let listed: Vec<AssetPath> = self
                .listing()
                .into_iter()
                .flat_map(|listing| &listing.handles)
                .filter_map(|file| self.asset_server.get_path(file.id()))
                .map(AssetPath::into_owned)
                .collect();
            self.data.broken.retain(|(path, _)| !listed.contains(path));
            // Those it has are listed. One it hasn't may be gone, or left
            // out by a scan begun before it loaded: loaded again to tell, as
            // Bevy doesn't reload a file removed (only its folder). Kept
            // until a listing has it, or it's not found.
            let asset_server = &self.asset_server;
            self.data.recovered.retain(|(path, _)| {
                let kept = !listed.contains(path);
                if kept {
                    asset_server.reload(path.clone());
                }
                kept
            });
        }
        if seen.fixed {
            // What loaded is indexed as it is until a listing has it.
            let (loaded, broken): (Vec<_>, Vec<_>) = core::mem::take(&mut self.data.broken)
                .into_iter()
                .partition(|(_, file)| self.asset_server.is_loaded(file));
            self.data.broken = broken;
            self.data.recovered.extend(loaded);
            if !seen.folder {
                // The folder's reload failed: asking for it again reloads
                // it, if it's failed still.
                self.data.handle = Some(self.asset_server.load_folder(self.data.path.clone()));
            }
        }
    }

    /// Its last listing, if it's been listed.
    fn listing(&self) -> Option<&LoadedFolder> {
        self.listings.get(self.handle()?)
    }

    /// Where the folder is: its files, once every one is settled (loaded,
    /// or failed to); `None` while one is loading, or the folder is listed
    /// again. Pushes to `errors` what's wrong. `reloading` once loaded.
    fn settle(&mut self, reloading: bool, errors: &mut DataErrors) -> Option<Settled> {
        match self.asset_server.load_state(self.handle()?) {
            LoadState::NotLoaded | LoadState::Loading => None,
            // Its last listing still holds every file it had: keep them.
            LoadState::Failed(error) if reloading => {
                errors.push(format!(
                    "failed to reload data folder {}, kept as it was: {error}",
                    self.data.path
                ));
                Some(Settled::Kept)
            }
            LoadState::Failed(error) => self.first_load_failed(&error, errors),
            LoadState::Loaded => {
                // Pushed once both are settled: tried again next frame if
                // not, they'd be pushed twice.
                let mut failed = Vec::new();
                let mut files = list(&self.asset_server, self.listing()?, &mut failed)?;
                self.add_recovered(&mut files, &mut failed)?;
                errors.extend(failed);
                files.sort_by(|(_, a), (_, b)| a.path().cmp(b.path()));
                Some(Settled::Files(files))
            }
        }
    }

    /// The folder failing to load on first load: listed again (once) if
    /// something went missing, `None`; otherwise why pushed to `errors`.
    fn first_load_failed(
        &mut self,
        error: &AssetLoadError,
        errors: &mut DataErrors,
    ) -> Option<Settled> {
        match error {
            // A file deleted between Bevy listing the folder and loading
            // it: listing it again leaves the file out. Once: one listed
            // but never there (a broken link), or the folder itself
            // missing, is an error.
            error if is_not_found(error) && !self.data.relisted => {
                self.data.relisted = true;
                self.data.handle = Some(self.asset_server.load_folder(self.data.path.clone()));
                return None;
            }
            // Bevy's `load_folder` skips a file no loader reads by
            // extension, but not this.
            AssetLoadError::MissingAssetLoader { asset_path, .. } => errors.push(format!(
                "{asset_path} is in data folder {}, but no asset loader reads it: move it \
                 out of the folder",
                self.data.path
            )),
            error => errors.push(format!(
                "failed to load data folder {}: {error}",
                self.data.path
            )),
        }
        Some(Settled::Failed)
    }

    /// Adds to `files`, a listing's, those recovered it hasn't: as a listed
    /// file is, its failure pushed to `failed`. `None` while one is still
    /// loading.
    fn add_recovered(
        &self,
        files: &mut Vec<(AssetId<DataFile>, AssetPath<'static>)>,
        failed: &mut Vec<String>,
    ) -> Option<()> {
        for (path, file) in &self.data.recovered {
            if files.iter().any(|(_, listed)| listed == path) {
                continue;
            }
            match self.asset_server.load_state(file) {
                LoadState::NotLoaded | LoadState::Loading => return None,
                LoadState::Failed(error) if is_not_found(&error) => continue,
                LoadState::Failed(error) => failed.push(error.to_string()),
                LoadState::Loaded => {}
            }
            files.push((file.id(), path.clone()));
        }
        Some(())
    }
}

/// Where the data folder settled.
enum Settled {
    /// Every file loaded or failed to: these, by path.
    Files(Vec<(AssetId<DataFile>, AssetPath<'static>)>),
    /// Its reload failed: its files kept as they were.
    Kept,
    /// Its first load failed.
    Failed,
}

/// The pipeline's own state, as systems use it: where loading is, whether
/// indexing is due, the data files, and what's wrong.
#[derive(SystemParam)]
pub(super) struct Pipeline<'w> {
    state: Res<'w, State<DataState>>,
    progress: ResMut<'w, Progress>,
    files: ResMut<'w, DataFiles>,
    errors: ResMut<'w, DataErrors>,
}

impl Pipeline<'_> {
    /// Whether the data's loaded: what changes is a reload.
    fn loaded(&self) -> bool {
        *self.state.get() == DataState::Loaded
    }

    /// Whether to see where the folder is: on first load, until indexing's
    /// due; once loaded, once something's changed (as `seen`) since it was
    /// last indexed.
    fn waiting(&mut self, seen: &Seen) -> bool {
        match self.state.get() {
            DataState::Loading => !self.progress.due,
            DataState::Loaded => {
                self.progress.dirty |= seen.any();
                self.progress.dirty
            }
            DataState::Failed => false,
        }
    }

    /// Takes where the folder settled: indexing's due.
    fn settled(&mut self, settled: Settled) {
        match settled {
            Settled::Files(files) => self.files.0 = files,
            Settled::Kept => {}
            Settled::Failed => self.files.0.clear(),
        }
        self.progress.dirty = false;
        self.progress.due = true;
    }

    /// Fails loading with `error`: indexing's due, to report it.
    fn fail(&mut self, error: String) {
        self.errors.push(error);
        self.progress.due = true;
    }
}

/// Starts loading the data folder.
pub(super) fn load(mut folder: Folder, mut pipeline: Pipeline) {
    if let Err(error) = folder.load() {
        pipeline.fail(error);
    }
}

/// The messages saying what changed among the data files: read every
/// frame, so what loading itself sent isn't taken for a change once loaded.
#[derive(SystemParam)]
pub(super) struct Changes<'w, 's> {
    files: MessageReader<'w, 's, AssetEvent<DataFile>>,
    folders: MessageReader<'w, 's, AssetEvent<LoadedFolder>>,
    failures: MessageReader<'w, 's, AssetLoadFailedEvent<DataFile>>,
    folder_failures: MessageReader<'w, 's, AssetLoadFailedEvent<LoadedFolder>>,
}

/// What changed this frame, of what the pipeline keeps. Each can with the
/// others, and each is acted on its own way.
#[derive(Debug, Default)]
#[expect(clippy::struct_excessive_bools, reason = "each can with the others")]
struct Seen {
    /// A data file was modified or removed, or failed to load again: to
    /// index again.
    files: bool,
    /// The folder was listed again: what it lists is fixed.
    folder: bool,
    /// The folder's reload failed.
    folder_failed: bool,
    /// The files its reloads failed on, as each failure said: to be held,
    /// though another reload may have listed the folder since.
    failing: Vec<AssetPath<'static>>,
    /// A file held as broken loaded: to be indexed.
    fixed: bool,
}

impl Seen {
    /// Whether anything did.
    fn any(&self) -> bool {
        self.files || self.folder || self.folder_failed || self.fixed
    }
}

impl Changes<'_, '_> {
    /// Reads every message, not up to the first that counts: one left
    /// unread would be taken for another change next frame. Only the data
    /// files count, `files`, and `folder`: the loader reads any file of its
    /// extensions the app loads, wherever it is. `Added` is left out: Bevy
    /// sends it for the first load's files a frame late, once loading is
    /// over; a file added later shows as the folder listed again.
    fn read(
        &mut self,
        files: &DataFiles,
        folder: &DataFolder,
        handle: AssetId<LoadedFolder>,
    ) -> Seen {
        let ours = |id: AssetId<DataFile>| files.0.iter().any(|(file, _)| *file == id);
        let broken = |id: AssetId<DataFile>| folder.broken.iter().any(|(_, file)| file.id() == id);
        let mut seen = Seen::default();
        for event in self.files.read() {
            match *event {
                AssetEvent::Modified { id } | AssetEvent::Removed { id } if ours(id) => {
                    seen.files = true;
                }
                AssetEvent::Added { id }
                | AssetEvent::Modified { id }
                | AssetEvent::LoadedWithDependencies { id }
                    if broken(id) =>
                {
                    seen.fixed = true;
                }
                _ => {}
            }
        }
        for failure in self.failures.read() {
            seen.files |= ours(failure.id);
        }
        for event in self.folders.read() {
            seen.folder |= event.is_modified(handle);
        }
        for failure in self.folder_failures.read() {
            if failure.id == handle {
                seen.folder_failed = true;
                seen.failing.extend(failing_file(&failure.error));
            }
        }
        seen
    }
}

/// Marks indexing due once every file is settled: on first load, once
/// they're all loaded (or failed to); once loaded, after one changed, was
/// added or removed. Lists the files, and what failed to load.
pub(super) fn settle(mut folder: Folder, mut messages: Changes, mut pipeline: Pipeline) {
    let Some(handle) = folder.handle() else {
        return;
    };
    let mut seen = messages.read(&pipeline.files, &folder.data, handle);
    // Added, or changed, and failing the folder's reload with it.
    if pipeline.loaded() && folder.hold_failing(&seen.failing) {
        seen.fixed = true;
    }
    folder.track(&seen);
    if !pipeline.waiting(&seen) {
        return;
    }
    let reloading = pipeline.loaded();
    if let Some(settled) = folder.settle(reloading, &mut pipeline.errors) {
        pipeline.settled(settled);
    }
}

/// Every data file of `folder`, by path; `None` while one is still
/// loading. Pushes to `failed` those that failed to.
fn list(
    asset_server: &AssetServer,
    folder: &LoadedFolder,
    failed: &mut Vec<String>,
) -> Option<Vec<(AssetId<DataFile>, AssetPath<'static>)>> {
    let mut files = Vec::new();
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
