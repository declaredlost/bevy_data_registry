//! Definitions files: which kind each holds ([`kind_of`]), and listing them
//! under the definitions' root.

use std::path::{Path, PathBuf};

use bevy::asset::AssetServerMode;
use bevy::asset::io::{AssetReaderError, AssetSourceId, ErasedAssetReader};
use bevy::prelude::*;
use bevy::tasks::futures_lite::StreamExt;

/// Which kind a definitions file holds, of the kinds' `folders`: the
/// first folder it's in, from the root, that's a kind's
/// (`abilities/bows.ron`, `effects/items/potions.ron`,
/// `monsters/spiders/hitboxes/net.ron`); in none, its name, if that's a
/// kind's or the end of one (`monsters/spiders/abilities.ron`,
/// `monsters/bandits/prefixes.ron`). `None` if neither: a file the
/// registry can't place. `path` is relative to the definitions' root; a
/// folder can be several deep (`affixes/prefixes`).
#[must_use]
pub fn kind_of<'a>(path: &Path, folders: &[&'a str]) -> Option<&'a str> {
    let in_folder = path
        .parent()
        .into_iter()
        .flat_map(|parent| {
            // Its folders from the root down: `a`, `a/b`, `a/b/c`.
            let mut deepest: Vec<&Path> = parent.ancestors().collect();
            deepest.reverse();
            deepest
        })
        .find_map(|folder| {
            folders
                .iter()
                .copied()
                .find(|kind| !folder.as_os_str().is_empty() && folder.ends_with(kind))
        });
    in_folder.or_else(|| {
        let name = path.file_stem()?.to_str()?;
        folders.iter().copied().find(|kind| {
            *kind == name || Path::new(kind).file_name().is_some_and(|last| last == name)
        })
    })
}

/// Returns the `.ron` files inside `folder` and its subfolders, sorted by
/// path.
///
/// Lists through the same reader the asset server loads from: the source
/// when unprocessed, the processed source when processed.
pub(crate) async fn list_ron_files(
    asset_server: &AssetServer,
    folder: &Path,
) -> Result<Vec<PathBuf>, AssetReaderError> {
    let source = asset_server
        .get_source(AssetSourceId::Default)
        .expect("default asset source is always registered");
    let reader: &dyn ErasedAssetReader = match asset_server.mode() {
        AssetServerMode::Unprocessed => source.reader(),
        AssetServerMode::Processed => source
            .processed_reader()
            .expect("processed mode always has a processed reader"),
    };
    let mut paths = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let mut stream = reader.read_directory(&folder).await?;
        while let Some(path) = stream.next().await {
            if path.extension().is_some_and(|ext| ext == "ron") {
                paths.push(path);
            } else if reader.is_directory(&path).await? {
                folders.push(path);
            }
        }
    }

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDERS: &[&str] = &[
        "abilities",
        "affixes/prefixes",
        "curios",
        "effects",
        "hitboxes",
        "items",
        "tables",
    ];

    fn kind(path: &str) -> Option<&'static str> {
        kind_of(Path::new(path), FOLDERS)
    }

    #[test]
    fn a_kind_folder_holds_its_kind_at_any_depth() {
        assert_eq!(kind("abilities/bows.ron"), Some("abilities"));
        assert_eq!(kind("items/ammo/arrows.ron"), Some("items"));
    }

    #[test]
    fn a_file_named_for_a_kind_holds_it_anywhere() {
        assert_eq!(kind("monsters/spiders/abilities.ron"), Some("abilities"));
        assert_eq!(kind("player/effects.ron"), Some("effects"));
    }

    #[test]
    fn the_first_kind_folder_wins_over_what_it_holds_or_its_name() {
        // Loot tables for curios; effects of items.
        assert_eq!(kind("tables/curios.ron"), Some("tables"));
        assert_eq!(kind("effects/items/potions.ron"), Some("effects"));
        assert_eq!(kind("monsters/spiders/hitboxes/net.ron"), Some("hitboxes"));
    }

    #[test]
    fn a_kind_can_be_folders_deep() {
        assert_eq!(
            kind("affixes/prefixes/damage.ron"),
            Some("affixes/prefixes")
        );
        assert_eq!(
            kind("monsters/bandits/prefixes.ron"),
            Some("affixes/prefixes")
        );
        assert_eq!(kind("affixes/notes.ron"), None);
    }

    #[test]
    fn a_file_of_no_kind_is_placed_nowhere() {
        assert_eq!(kind("monsters/spiders/notes.ron"), None);
        assert_eq!(kind("readme.ron"), None);
    }
}
