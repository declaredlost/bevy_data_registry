//! Checking references: every [`Id`](crate::Id) in a definition, but its
//! own and its base, names a definition
//! [`Definitions::get`](crate::Definitions::get) finds.

use bevy::prelude::*;

use super::DataErrors;
use crate::file::{DataFile, Kinds};
use crate::registry::Target;

/// Pushes to [`DataErrors`] every [`Id`](crate::Id) in a definition, but
/// its own and its base, that names no definition `get` finds: of what the
/// registry gives (whole, or kept from before a reload) and of the families
/// as written, once every kind is indexed. One naming a definition whose
/// base doesn't resolve is skipped: that's reported already.
pub(super) fn check(world: &mut World) {
    let kinds = world.resource::<Kinds>().clone();
    let kinds = kinds.read();
    let assets = world.resource::<Assets<DataFile>>();
    let mut errors = Vec::new();
    for kind in kinds.iter() {
        for from in (kind.referenced)(world) {
            for reference in from.references {
                let to = kinds.iter().find(|to| to.id == reference.kind);
                let lookup = to.map(|to| (to, (to.lookup)(world, reference.hash)));
                if let Some((_, Some((Target::Read | Target::Unresolved, _)))) = lookup {
                    continue;
                }
                // By the name its kind read it as, or its file did.
                let named = lookup
                    .as_ref()
                    .and_then(|(_, found)| found.as_ref()?.1.as_deref())
                    .map(crate::id::escaped);
                let name = named.unwrap_or_else(|| {
                    assets.get(from.file).map_or_else(
                        || format!("#{:016x}", reference.hash),
                        |file| file.names().show(reference.kind, reference.hash),
                    )
                });
                let problem = match lookup {
                    Some((to, Some((Target::Family, _)))) => format!(
                        "refers to {} `{name}`, a family: families are only built on",
                        to.name
                    ),
                    Some((to, Some(_))) => {
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
                let source = if from.kept {
                    format!("kept from {}", from.path)
                } else {
                    from.path.to_string()
                };
                errors.push(format!(
                    "{} `{}` ({source}): `{}` {problem}",
                    kind.name, from.id, reference.field
                ));
            }
        }
    }
    world.resource_mut::<DataErrors>().extend(errors);
}
