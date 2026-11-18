//! Indexing a kind: every definition by id, those that build on a family
//! made whole.

use bevy::prelude::*;

use super::DataErrors;
use super::load::DataFiles;
use crate::Definition;
use crate::file::DataFile;
use crate::registry::Registry;

/// Indexes every definition of kind `T`, from every file, and makes those
/// that build on a family whole: rejecting ids used twice and families
/// that don't resolve.
pub(super) fn index_kind<T: Definition>(
    files: Res<DataFiles>,
    assets: Res<Assets<DataFile>>,
    mut registry: ResMut<Registry<T>>,
    mut errors: ResMut<DataErrors>,
) {
    errors.extend(registry.index(&files.0, &assets));
}
