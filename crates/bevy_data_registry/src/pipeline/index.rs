//! Indexing a kind: every definition by id, those that build on a family
//! made whole.

use bevy::prelude::*;

use super::DataErrors;
use crate::registry::Registry;
use crate::{DataAsset, Definition};

/// Indexes every loaded definition of kind `T` by id, from all its files,
/// and makes those that build on a family whole: rejecting ids used twice
/// and families that don't resolve.
pub(super) fn index_kind<T: Definition>(
    asset_server: Res<AssetServer>,
    assets: Res<Assets<DataAsset<T>>>,
    mut registry: ResMut<Registry<T>>,
    mut errors: ResMut<DataErrors>,
) {
    errors.extend(registry.index(&asset_server, &assets));
}
