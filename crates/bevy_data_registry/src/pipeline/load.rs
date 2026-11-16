//! First load: listing every file by the kind it holds, loading each
//! kind's, and waiting for them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy::asset::LoadState;
use bevy::asset::io::AssetReaderError;
use bevy::prelude::*;
use bevy::tasks::block_on;

use super::{DataErrors, Layout};
use crate::files::{kind_of, list_ron_files};
use crate::registry::Registry;
use crate::{DataAsset, Definition};

/// Every definitions file, by the kind it holds: listed once, before each
/// kind loads its own.
#[derive(Resource, Debug, Default)]
pub(crate) struct Files(BTreeMap<&'static str, Vec<PathBuf>>);

/// The files being loaded, until every one is.
#[derive(Resource, Default)]
pub(crate) struct Pending {
    handles: Vec<UntypedHandle>,
    /// Whether every one is (or failed to): indexing is next.
    pub loaded: bool,
}

impl Pending {
    /// How many files there were.
    pub fn count(&self) -> usize {
        self.handles.len()
    }
}

/// Lists every `.ron` file under `assets/<root>/`, at any depth, and
/// places each by [`kind_of`]: the first kind's folder it's in, or its
/// name. A file of no kind is an error. Subfolders otherwise only organise
/// the files.
pub(super) fn list(
    asset_server: Res<AssetServer>,
    layout: Res<Layout>,
    mut files: ResMut<Files>,
    mut errors: ResMut<DataErrors>,
) {
    let root = &layout.root;
    let paths = match block_on(list_ron_files(&asset_server, root)) {
        Ok(paths) => paths,
        Err(AssetReaderError::NotFound(_)) => {
            warn!("definitions folder {} not found", root.display());
            return;
        }
        Err(err) => {
            errors.push(format!("failed to list {}: {err}", root.display()));
            return;
        }
    };
    for path in paths {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        match kind_of(relative, &layout.folders) {
            Some(kind) => files.0.entry(kind).or_default().push(path),
            None => errors.push(format!(
                "{}: not a kind's file: name it after one, or put it in a kind's folder ({})",
                path.display(),
                layout.folders.join(", ")
            )),
        }
    }
}

/// Loads every file of kind `T`, as listed. Always typed: every kind
/// shares the `.ron` extension, so an untyped load would pick the wrong
/// loader.
pub(super) fn load_kind<T: Definition>(
    asset_server: Res<AssetServer>,
    files: Res<Files>,
    mut registry: ResMut<Registry<T>>,
    mut pending: ResMut<Pending>,
) {
    for path in files.0.get(T::FOLDER).into_iter().flatten() {
        let handle = asset_server.load::<DataAsset<T>>(path.clone());
        pending.handles.push(handle.clone().untyped());
        registry.queue(handle);
    }
}

/// Marks the files loaded once every one is, or one failed (recorded, and
/// reported as loading finishes).
pub(super) fn wait(
    asset_server: Res<AssetServer>,
    mut pending: ResMut<Pending>,
    mut errors: ResMut<DataErrors>,
) {
    if pending.loaded {
        return;
    }
    let mut loading = false;
    for handle in &pending.handles {
        match asset_server.load_state(handle) {
            LoadState::Loaded => {}
            LoadState::Failed(err) => errors.push(err.to_string()),
            LoadState::NotLoaded | LoadState::Loading => loading = true,
        }
    }
    if !errors.is_empty() || !loading {
        pending.loaded = true;
    }
}
