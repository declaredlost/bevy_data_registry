//! How loading ends, and each reload.

use bevy::prelude::*;

use super::load::{DataFiles, Progress};
use super::{DataErrors, DataState};

/// Sent once the definitions are indexed and validated after a hot reload.
///
/// A file changed, was added or was removed while the app runs (with
/// `dev`): whatever was built from the definitions should be built again.
#[derive(Message, Debug, Clone, Copy)]
pub struct DataReloaded;

/// Ends the first load: loaded, if nothing's wrong; otherwise every
/// problem logged, and failed ([`exit`] then ends the app).
pub(super) fn finish_load(
    files: Res<DataFiles>,
    mut progress: ResMut<Progress>,
    mut errors: ResMut<DataErrors>,
    mut state: ResMut<NextState<DataState>>,
) {
    progress.due = false;
    if errors.is_empty() {
        info!("loaded {} data files", files.0.len());
        state.set(DataState::Loaded);
    } else {
        for error in errors.drain() {
            error!("{error}");
        }
        state.set(DataState::Failed);
    }
}

/// Tells the app to exit, as loading failed: on entering
/// [`DataState::Failed`], so the app's own systems there run first.
pub(super) fn exit(mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::error());
}

/// Ends a reload: its problems logged rather than stopping the app, which
/// is only being worked on, and the game told.
pub(super) fn finish_reload(
    mut progress: ResMut<Progress>,
    mut errors: ResMut<DataErrors>,
    mut reloaded: MessageWriter<DataReloaded>,
) {
    progress.due = false;
    let mut count = 0;
    for error in errors.drain() {
        error!("{error}");
        count += 1;
    }
    reloaded.write(DataReloaded);
    match count {
        0 => info!("reloaded definitions"),
        1 => warn!("reloaded definitions with a problem"),
        count => warn!("reloaded definitions with {count} problems"),
    }
}
