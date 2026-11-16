//! How loading ends, and each reload: what changed, and what was found.

use bevy::prelude::*;

use super::load::Pending;
use super::{DataErrors, DataState};
use crate::registry::Registry;
use crate::{DataAsset, Definition};

/// Sent when a file changed while the app runs and the definitions were
/// indexed and validated again: whatever was built from them should be
/// built again.
#[derive(Message, Debug, Clone, Copy)]
pub struct DataReloaded;

/// Whether a file changed since the definitions were last validated.
#[derive(Resource, Debug, Default)]
pub(crate) struct Reload {
    pub pending: bool,
}

/// Ends the first load: loaded, if nothing's wrong; otherwise every
/// problem logged, failed, and the app told to exit.
pub(super) fn finish_load(
    pending: Res<Pending>,
    mut errors: ResMut<DataErrors>,
    mut state: ResMut<NextState<DataState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if errors.is_empty() {
        info!("loaded {} definition files", pending.count());
        state.set(DataState::Loaded);
    } else {
        for error in errors.drain() {
            error!("{error}");
        }
        state.set(DataState::Failed);
        exit.write(AppExit::error());
    }
}

/// Notes when one of kind `T`'s files is reloaded, its definitions
/// already updated in place.
pub(super) fn watch_kind<T: Definition>(
    mut events: MessageReader<AssetEvent<DataAsset<T>>>,
    registry: Res<Registry<T>>,
    mut reload: ResMut<Reload>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event
            && registry.has_file(*id)
        {
            reload.pending = true;
        }
    }
}

/// Ends a reload: its problems logged rather than stopping the app, which
/// is only being worked on, and the game told.
pub(super) fn finish_reload(
    mut errors: ResMut<DataErrors>,
    mut reload: ResMut<Reload>,
    mut reloaded: MessageWriter<DataReloaded>,
) {
    let mut count = 0;
    for error in errors.drain() {
        error!("{error}");
        count += 1;
    }
    reload.pending = false;
    reloaded.write(DataReloaded);
    match count {
        0 => info!("reloaded definitions"),
        1 => warn!("reloaded definitions with a problem"),
        count => warn!("reloaded definitions with {count} problems"),
    }
}
