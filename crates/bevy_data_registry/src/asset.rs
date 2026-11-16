use core::any::type_name;
use core::fmt;
use core::marker::PhantomData;

use bevy::prelude::*;
use bevy::reflect::utility::GenericTypePathCell;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::de::value::MapAccessDeserializer;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::Definition;

/// Bevy asset holding the definitions of one file: a single definition,
/// `(id: ..)`, or a list of them, `[(id: ..), ..]`.
///
/// Definition types needn't depend on Bevy, so they needn't implement
/// [`Asset`] themselves. Read the definitions through
/// [`Defs`](crate::Defs).
#[derive(Asset, Debug, Clone)]
pub struct DataAsset<T: Definition>(pub Vec<T>);

/// Reads a file as one definition or a list of them, keeping each
/// definition's own errors (and their positions) intact.
struct DataVisitor<T>(PhantomData<T>);

// `#[derive(TypePath)]` would require `T: TypePath`, which definition
// types needn't implement.
impl<T: Definition> TypePath for DataAsset<T> {
    fn type_path() -> &'static str {
        static CELL: GenericTypePathCell = GenericTypePathCell::new();
        CELL.get_or_insert::<Self, _>(|| {
            format!("bevy_data_registry::DataAsset<{}>", type_name::<T>())
        })
    }

    fn short_type_path() -> &'static str {
        static CELL: GenericTypePathCell = GenericTypePathCell::new();
        CELL.get_or_insert::<Self, _>(|| {
            let name = type_name::<T>().rsplit("::").next().unwrap_or_default();
            format!("DataAsset<{name}>")
        })
    }

    fn type_ident() -> Option<&'static str> {
        Some("DataAsset")
    }

    fn crate_name() -> Option<&'static str> {
        Some("bevy_data_registry")
    }

    fn module_path() -> Option<&'static str> {
        Some("bevy_data_registry")
    }
}

impl<'de, T: Definition> Deserialize<'de> for DataAsset<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(DataVisitor(PhantomData))
            .map(Self)
    }
}

impl<'de, T: Definition> Visitor<'de> for DataVisitor<T> {
    type Value = Vec<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a definition or a list of definitions")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut defs = Vec::with_capacity(seq.size_hint().unwrap_or_default());
        while let Some(def) = seq.next_element()? {
            defs.push(def);
        }
        Ok(defs)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        T::deserialize(MapAccessDeserializer::new(map)).map(|def| vec![def])
    }
}

/// Registers the RON loader of a definition kind, for no extension.
///
/// Every kind shares `.ron`, so a loader picked by extension would be
/// whichever kind registered last. Definitions are only loaded typed,
/// where Bevy picks the loader by asset type; but when a hot reload fails
/// (a typo), Bevy retries it untyped, by extension, and would parse the
/// file as the wrong kind.
pub(crate) fn add_loader<T: Definition>(app: &mut App, options: ron::Options) {
    app.add_plugins(RonAssetPlugin::<DataAsset<T>>::new(&[]).with_options(options));
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Tag {
        id: String,
    }

    impl Definition for Tag {
        type Id = String;

        const FOLDER: &'static str = "tags";
        const NAME: &'static str = "tag";

        fn id(&self) -> &String {
            &self.id
        }
    }

    fn ids(source: &str) -> Vec<String> {
        let DataAsset(defs) = ron::from_str::<DataAsset<Tag>>(source).unwrap();
        defs.into_iter().map(|tag| tag.id).collect()
    }

    #[test]
    fn a_file_holds_one_definition_or_a_list() {
        assert_eq!(ids(r#"(id: "fire")"#), ["fire"]);
        assert_eq!(
            ids(r#"[(id: "fire"), (id: "physical")]"#),
            ["fire", "physical"]
        );
    }

    #[test]
    fn errors_inside_a_list_keep_their_position() {
        let error = ron::from_str::<DataAsset<Tag>>("[\n    (id: \"fire\"),\n    (idd: \"x\"),\n]")
            .unwrap_err();
        assert_eq!(error.span.start.line, 3, "{error}");
    }
}
