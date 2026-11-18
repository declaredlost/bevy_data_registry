//! Checking references: every [`Id`](crate::Id) in a definition but its
//! own names a definition of its kind.

use bevy::prelude::*;

use super::DataErrors;
use super::load::DataFiles;
use crate::file::{DataFile, Kinds};

/// Pushes to [`DataErrors`] every [`Id`](crate::Id) in a definition, but
/// its own, that names no definition of its kind: of every file, once
/// every kind is indexed.
pub(super) fn check(world: &mut World) {
    let kinds = world.resource::<Kinds>().clone();
    let kinds = kinds.read();
    let files = world.resource::<DataFiles>();
    let assets = world.resource::<Assets<DataFile>>();
    let mut errors = Vec::new();
    for (file, path) in &files.0 {
        let Some(file) = assets.get(*file) else {
            continue;
        };
        for (kind, id, references) in file.references(&kinds) {
            for reference in references {
                let to = kinds.iter().find(|to| to.id == reference.kind);
                let found = to.map(|to| (to, (to.contains)(world, reference.hash)));
                if let Some((_, Some(true))) = found {
                    continue;
                }
                let name = file.names().show(reference.kind, reference.hash);
                let problem = match found {
                    Some((_, Some(true))) => continue,
                    Some((to, Some(false))) => {
                        format!("refers to {} `{name}`, which isn't there", to.name)
                    }
                    Some((to, None)) => format!(
                        "refers to {0} `{name}` by `Id<{0}>`, but {0}'s ids aren't `Id`s",
                        to.name
                    ),
                    None => format!(
                        "refers to {0} `{name}`, but {0} isn't a kind the app registers",
                        reference.kind_name
                    ),
                };
                errors.push(format!(
                    "{} `{id}` ({path}): `{}` {problem}",
                    kind.name, reference.field
                ));
            }
        }
    }
    world.resource_mut::<DataErrors>().extend(errors);
}
