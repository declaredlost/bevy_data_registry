//! How loading ends, and each reload.

use bevy::prelude::*;

use super::load::{DataFiles, Progress};
use super::{DataErrors, DataState};

/// Sent once the definitions are indexed and validated after a hot reload.
///
/// A file changed, was added or was removed while the app runs (with
/// `dev`): whatever was built from the definitions should be built again.
/// It marks the end of the pass, not that it passed: what was wrong was
/// logged, and the definitions read as indexed.
/// Read it in `PostUpdate`, after the pipeline in `Update`: read in
/// `Update`, it may be read a frame late, as the next reload starts.
#[derive(Message, Debug, Clone, Copy)]
pub struct DataReloaded;

/// Ends the first load: loaded, if nothing's wrong; otherwise every
/// problem logged, and kept in [`DataErrors`], and failed ([`exit`] then
/// ends the app, unless that's off).
pub(super) fn finish_load(
    files: Res<DataFiles>,
    mut progress: ResMut<Progress>,
    errors: Res<DataErrors>,
    mut state: ResMut<NextState<DataState>>,
) {
    progress.due = false;
    if errors.is_empty() {
        info!("loaded {} data files", files.0.len());
        state.set(DataState::Loaded);
    } else {
        for error in errors.iter() {
            error!("{error}");
        }
        state.set(DataState::Failed);
    }
}

/// Tells the app to exit, as loading failed: in `Last`, once in
/// [`DataState::Failed`], so the app's own systems on entering it run
/// first.
pub(super) fn exit(mut sent: Local<bool>, mut exit: MessageWriter<AppExit>) {
    if !core::mem::replace(&mut *sent, true) {
        exit.write(AppExit::error());
    }
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
