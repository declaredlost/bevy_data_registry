//! Data files: [`DataFile`], the definitions of one file; the kinds the
//! loader knows, and which files each claims; the loader, reading a file
//! as its kind in its [`DataFormat`].

use core::any::{Any, TypeId};
use core::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, AssetPath, LoadContext};
use bevy::prelude::*;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use crate::Definition;
use crate::format::DataFormat;
use crate::id::{self, Id, Names};
use crate::registry::{Referencing, Registry, Target};
use crate::resolve::Resolve;

/// The definitions read from one data file, all of the kind that claims it.
///
/// A file holds one, `(id: ..)`, or a list of them, `[(id: ..), ..]`.
///
/// Read definitions through [`Definitions`](crate::Definitions), which
/// indexes every file's.
#[derive(Asset, TypePath)]
pub struct DataFile {
    entries: Vec<Entry>,
    /// The names its [`Id`]s were read from, for messages.
    names: Names,
}

/// One definition of a file, its kind erased: shared with the registry,
/// which keeps what it indexed when the file is loaded again.
struct Entry {
    kind: TypeId,
    value: Arc<dyn Any + Send + Sync>,
}

impl DataFile {
    /// How many definitions it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The names its [`Id`]s were read from.
    pub(crate) fn names(&self) -> &Names {
        &self.names
    }

    /// Its definitions of kind `T`, with their positions in the file.
    pub(crate) fn of<T: Definition>(&self) -> impl Iterator<Item = (usize, &T)> {
        self.entries.iter().enumerate().filter_map(|(at, entry)| {
            (entry.kind == TypeId::of::<T>())
                .then(|| entry.value.downcast_ref::<T>())
                .flatten()
                .map(|def| (at, def))
        })
    }

    /// A file of `defs`, as if read, with `names` read in it.
    #[cfg(test)]
    pub(crate) fn of_defs_named<T: Definition>(defs: Vec<T>, names: Names) -> Self {
        Self {
            names,
            ..Self::of_defs(defs)
        }
    }

    /// A file of `defs`, as if read.
    #[cfg(test)]
    pub(crate) fn of_defs<T: Definition>(defs: Vec<T>) -> Self {
        let entries = defs
            .into_iter()
            .map(|def| Entry {
                kind: TypeId::of::<T>(),
                value: Arc::new(def),
            })
            .collect();
        Self {
            entries,
            names: Names::default(),
        }
    }

    /// Its definition at `at`, if it's of kind `T`.
    pub(crate) fn get<T: Definition>(&self, at: usize) -> Option<&T> {
        self.entries.get(at)?.value.downcast_ref()
    }

    /// Its definition at `at`, shared, if it's of kind `T`.
    pub(crate) fn shared<T: Definition>(&self, at: usize) -> Option<Arc<T>> {
        self.entries.get(at)?.value.clone().downcast().ok()
    }
}

impl fmt::Debug for DataFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataFile")
            .field("len", &self.entries.len())
            .finish_non_exhaustive()
    }
}

/// The kinds the app knows: shared by the app, which registers them, and
/// the loader, which reads each file as the kind that claims it.
#[derive(Resource, Clone, Default)]
pub(crate) struct Kinds(Arc<RwLock<Vec<Kind>>>);

impl Kinds {
    /// Every kind, read.
    pub(crate) fn read(&self) -> std::sync::RwLockReadGuard<'_, Vec<Kind>> {
        self.0.read().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A kind, to the loader: the files it claims, and how to read one of
/// its definitions.
pub(crate) struct Kind {
    pub id: TypeId,
    pub name: &'static str,
    resolvers: Vec<Box<dyn Resolve>>,
    deserialize: DeserializeFn,
    /// The references in every definition of the kind, as read: what
    /// [`Definitions`](crate::Definitions) gives, families and those whose
    /// bases don't resolve as written.
    pub referenced: fn(&World) -> Vec<Referencing>,
    /// What an [`Id`](crate::Id) of the kind with this hash names, and its
    /// name if read; `None` if the kind's ids aren't `Id`s.
    pub lookup: fn(&World, u64) -> Option<(Target, Option<String>)>,
}

type DeserializeFn = fn(
    &mut dyn erased_serde::Deserializer<'_>,
) -> Result<Box<dyn Any + Send + Sync>, erased_serde::Error>;

impl Kind {
    /// The rule by which the file at `path` is this kind's, if it is.
    fn claim(&self, path: &Path) -> Option<&dyn Resolve> {
        self.resolvers
            .iter()
            .map(AsRef::as_ref)
            .find(|rule| rule.claims(path))
    }
}

impl Kinds {
    /// Adds kind `T`, named `name`, whose files `resolvers` claim. `false`
    /// if it already was.
    ///
    /// # Panics
    ///
    /// If `resolvers` is empty: whether any files match isn't checked.
    pub fn add<T: Definition>(&self, name: &'static str, resolvers: Vec<Box<dyn Resolve>>) -> bool {
        let mut kinds = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if kinds.iter().any(|kind| kind.id == TypeId::of::<T>()) {
            return false;
        }
        assert!(
            !resolvers.is_empty(),
            "{name} claims no data files: say which with #[definition(file = \"..\")], `folder`, `extension` or `resolve`",
        );
        kinds.push(Kind {
            id: TypeId::of::<T>(),
            name,
            resolvers,
            deserialize: |deserializer| {
                erased_serde::deserialize::<T>(deserializer)
                    .map(|def| Box::new(def) as Box<dyn Any + Send + Sync>)
            },
            referenced: |world| {
                let (Some(registry), Some(assets)) = (
                    world.get_resource::<Registry<T>>(),
                    world.get_resource::<Assets<DataFile>>(),
                ) else {
                    return Vec::new();
                };
                registry.referenced(assets)
            },
            lookup: |world, hash| {
                let id: &dyn Any = &Id::<T>::from_hash(hash);
                let id = id.downcast_ref::<T::Id>()?;
                let registry = world.get_resource::<Registry<T>>()?;
                Some((
                    registry.target(id),
                    registry.name(id).map(ToOwned::to_owned),
                ))
            },
        });
        true
    }
}

/// Reads a data file as the kind that claims it, on a format's deserializer.
///
/// Given to [`DataFormat::read`], which runs it on its deserializer: it
/// reads one definition, or a list of them.
pub struct FileSeed<'a>(&'a Kind);

/// What a [`FileSeed`] read from a file: its definitions, of the one kind.
pub struct FileContents(Vec<Entry>);

impl<'de> DeserializeSeed<'de> for FileSeed<'_> {
    type Value = FileContents;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<FileContents, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for FileSeed<'_> {
    type Value = FileContents;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a {} or a list of them", self.0.name)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<FileContents, A::Error> {
        let mut entries = Vec::with_capacity(seq.size_hint().unwrap_or_default());
        while let Some(entry) = seq.next_element_seed(OneSeed(self.0))? {
            entries.push(entry);
        }
        Ok(FileContents(entries))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<FileContents, A::Error> {
        let entry = OneSeed(self.0).deserialize(MapAccessDeserializer::new(map))?;
        Ok(FileContents(vec![entry]))
    }
}

/// One definition of a kind.
#[derive(Clone, Copy)]
struct OneSeed<'a>(&'a Kind);

impl<'de> DeserializeSeed<'de> for OneSeed<'_> {
    type Value = Entry;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Entry, D::Error> {
        let mut deserializer = <dyn erased_serde::Deserializer>::erase(deserializer);
        // Errors from inside a definition keep their message; a format
        // that tracks positions keeps its own, as reading stops there.
        let value = (self.0.deserialize)(&mut deserializer).map_err(de::Error::custom)?;
        Ok(Entry {
            kind: self.0.id,
            value: Arc::from(value),
        })
    }
}

/// Loads a [`DataFile`] in any of the app's formats, as the kind that
/// claims it.
#[derive(TypePath)]
pub(crate) struct DataLoader {
    pub kinds: Kinds,
    /// The data folder: kinds claim files by their path in it.
    pub root: AssetPath<'static>,
    pub formats: Vec<Arc<dyn DataFormat>>,
    /// Every format's extensions, for Bevy.
    pub extensions: Vec<&'static str>,
}

/// Why a data file couldn't be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DataFileError {
    /// It couldn't be read.
    #[error("failed to read the file: {0}")]
    Io(#[from] std::io::Error),
    /// It isn't valid in its format, or not of its kind.
    #[error("{0}")]
    Format(BevyError),
    /// No kind claims it.
    #[error("no kind claims this file; they claim {0}")]
    Unclaimed(String),
    /// Two kinds claim it.
    #[error("both {0} ({1}) and {2} ({3}) claim this file: give one other files")]
    Claimed(&'static str, String, &'static str, String),
    /// It's outside the data folder.
    #[error("not in the data folder, so no kind's")]
    Outside,
}

impl AssetLoader for DataLoader {
    type Asset = DataFile;
    type Error = DataFileError;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<DataFile, DataFileError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let relative = in_data(&self.root, load_context.path()).ok_or(DataFileError::Outside)?;
        let format = self.format_of(&relative);
        let kinds = self.kinds.0.read().unwrap_or_else(PoisonError::into_inner);
        parse(format, &relative, &bytes, &kinds)
    }

    fn extensions(&self) -> &[&str] {
        &self.extensions
    }
}

/// `path` from the data folder at `root`, without a query after its
/// extension (`tags.json?rev=2`), as Bevy picks loaders; `None` if it's not
/// in it: another source, another folder, or a way out of it
/// (`data/../items.ron`).
pub(crate) fn in_data(root: &AssetPath, path: &AssetPath) -> Option<PathBuf> {
    if path.source() != root.source() {
        return None;
    }
    let relative = path.path().strip_prefix(root.path()).ok()?;
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let mut relative = relative.to_path_buf();
    // As Bevy's `AssetPath::get_full_extension`: a `?` after the first dot.
    let query = relative
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| {
            let dot = name.find('.')?;
            let query = name[dot..].find('?')?;
            Some(name[..dot + query].to_owned())
        });
    if let Some(name) = query {
        relative.set_file_name(name);
    }
    Some(relative)
}

impl DataLoader {
    /// The format of the file at `path`: the one with the longest
    /// extension it ends in; the first if none, as Bevy only gives this
    /// loader files of theirs.
    fn format_of(&self, path: &Path) -> &dyn DataFormat {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let by_extension = self
            .formats
            .iter()
            .flat_map(|format| format.extensions().iter().map(move |ext| (format, *ext)))
            .filter(|(_, ext)| {
                name.strip_suffix(ext)
                    .is_some_and(|rest| rest.ends_with('.'))
            })
            .max_by_key(|(_, ext)| ext.len())
            .map(|(format, _)| format);
        by_extension
            .or_else(|| self.formats.first())
            .expect("the plugin has a format")
            .as_ref()
    }
}

/// Reads `bytes`, the file at `path` in the data folder, in `format`, as
/// the one kind that claims it.
fn parse(
    format: &dyn DataFormat,
    path: &Path,
    bytes: &[u8],
    kinds: &[Kind],
) -> Result<DataFile, DataFileError> {
    let claiming: Vec<(&Kind, &dyn Resolve)> = kinds
        .iter()
        .filter_map(|kind| Some((kind, kind.claim(path)?)))
        .collect();
    let kind = match claiming[..] {
        [(kind, _)] => kind,
        [] => {
            let rules: Vec<String> = kinds
                .iter()
                .flat_map(|kind| {
                    kind.resolvers
                        .iter()
                        .map(move |rule| format!("{rule} ({})", kind.name))
                })
                .collect();
            return Err(DataFileError::Unclaimed(rules.join(", ")));
        }
        [(first, first_rule), (second, second_rule), ..] => {
            return Err(DataFileError::Claimed(
                first.name,
                first_rule.to_string(),
                second.name,
                second_rule.to_string(),
            ));
        }
    };
    let (read, names) = id::naming(|| format.read(bytes, FileSeed(kind)));
    let FileContents(entries) = read.map_err(DataFileError::Format)?;
    Ok(DataFile { entries, names })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::format::Ron;

    #[derive(crate::Definition, Debug, Deserialize)]
    #[definition(file = "tags")]
    #[serde(deny_unknown_fields)]
    struct Tag {
        id: String,
    }

    #[derive(crate::Definition, Debug, Deserialize)]
    #[definition(file = "items", folder = "items")]
    struct Item {
        #[definition(id)]
        name: String,
    }

    /// Adds kind `T` to `kinds`, as registering it does.
    fn add<T: Definition>(kinds: &Kinds) -> bool {
        let info = T::info();
        kinds.add::<T>(info.name, info.resolvers)
    }

    fn kinds() -> Kinds {
        let kinds = Kinds::default();
        add::<Tag>(&kinds);
        add::<Item>(&kinds);
        kinds
    }

    /// `source`, read as the RON file at `path` in the data folder.
    fn parse_at(path: &str, source: &str) -> Result<DataFile, DataFileError> {
        let kinds = kinds();
        let kinds = kinds.0.read().unwrap();
        super::parse(&Ron::default(), Path::new(path), source.as_bytes(), &kinds)
    }

    fn line(error: DataFileError) -> usize {
        let DataFileError::Format(error) = error else {
            panic!("not a format error: {error}");
        };
        let error = error
            .downcast_ref::<ron::error::SpannedError>()
            .expect("a RON error");
        error.span.start.line
    }

    #[test]
    fn a_file_holds_one_definition_or_a_list_of_its_kind() {
        let one = parse_at("enemies/spider/tags.ron", r#"(id: "fire")"#).unwrap();
        let tags: Vec<&str> = one.of::<Tag>().map(|(_, tag)| tag.id.as_str()).collect();
        assert_eq!(tags, ["fire"]);
        let many = parse_at("items/weapons.ron", r#"[(name: "sword"), (name: "bow")]"#).unwrap();
        let names: Vec<&str> = many
            .of::<Item>()
            .map(|(_, item)| item.name.as_str())
            .collect();
        assert_eq!(names, ["sword", "bow"]);
        assert_eq!(
            many.get::<Item>(1).map(|item| item.name.as_str()),
            Some("bow")
        );
        assert!(many.get::<Tag>(0).is_none(), "not a tag");
        assert!(parse_at("tags.ron", "[]").unwrap().is_empty());
    }

    #[test]
    fn errors_keep_their_position() {
        let error =
            parse_at("tags.ron", "[\n    (id: \"fire\"),\n    (idd: \"x\"),\n]").unwrap_err();
        assert_eq!(line(error), 3);
        let error = parse_at("tags.ron", "(\n    idd: \"x\",\n)").unwrap_err();
        assert_eq!(line(error), 2);
    }

    #[test]
    fn a_file_no_kind_claims_lists_what_they_do() {
        let error = parse_at("enemies/notes.ron", "[]").unwrap_err();
        let message = error.to_string();
        assert!(matches!(error, DataFileError::Unclaimed(_)), "{message}");
        assert!(
            message.contains("files named `tags` (Tag)")
                && message.contains("files under `items/` (Item)"),
            "{message}"
        );
    }

    #[test]
    fn a_file_two_kinds_claim_is_an_error() {
        let error = parse_at("items/tags.ron", "[]").unwrap_err();
        assert!(
            matches!(&error, DataFileError::Claimed("Tag", _, "Item", _)),
            "{error}"
        );
        assert_eq!(
            error.to_string(),
            "both Tag (files named `tags`) and Item (files under `items/`) claim this file: give one other files"
        );
    }

    #[test]
    fn two_kinds_with_one_rule_fail_on_its_files() {
        #[derive(crate::Definition, Deserialize)]
        #[definition(folder = "others", file = "tags")]
        struct Other {
            id: u32,
        }
        let kinds = kinds();
        assert!(add::<Other>(&kinds), "registering it doesn't panic");
        let kinds = kinds.0.read().unwrap();
        let error =
            super::parse(&Ron::default(), Path::new("tags.ron"), b"[]", &kinds).unwrap_err();
        assert!(
            matches!(&error, DataFileError::Claimed("Tag", _, "Other", _)),
            "{error}"
        );
    }

    #[test]
    fn a_format_is_picked_by_its_longest_extension() {
        struct Plain(&'static [&'static str]);
        impl DataFormat for Plain {
            fn extensions(&self) -> &[&'static str] {
                self.0
            }

            fn read(&self, _: &[u8], _: FileSeed<'_>) -> Result<FileContents, BevyError> {
                Ok(FileContents(Vec::new()))
            }
        }
        let loader = DataLoader {
            kinds: Kinds::default(),
            root: AssetPath::from("data"),
            formats: vec![Arc::new(Plain(&["ron"])), Arc::new(Plain(&["data.ron"]))],
            extensions: Vec::new(),
        };
        let extensions = |path: &str| loader.format_of(Path::new(path)).extensions();
        assert_eq!(extensions("items.ron"), ["ron"]);
        assert_eq!(extensions("items.data.ron"), ["data.ron"]);
        assert_eq!(extensions("items.json"), ["ron"], "the first otherwise");
    }

    #[test]
    fn only_paths_in_the_data_folder_are_data() {
        let root = AssetPath::from("data");
        let in_data = |path: &str| {
            super::in_data(&root, &AssetPath::parse(path)).map(|path| path.display().to_string())
        };
        assert_eq!(in_data("data/items.ron").as_deref(), Some("items.ron"));
        assert_eq!(
            in_data("data/spider/tags.json?rev=2").as_deref(),
            Some("spider/tags.json"),
            "without the query"
        );
        assert_eq!(
            in_data("data/what?.items.ron").as_deref(),
            Some("what?.items.ron"),
            "a `?` before the extension is the name's"
        );
        assert_eq!(in_data("items.ron"), None);
        assert_eq!(in_data("data/../items.ron"), None, "a way out");
        assert_eq!(
            in_data("data/./items.ron").as_deref(),
            Some("items.ron"),
            "the same file"
        );
        assert_eq!(in_data("mods://data/items.ron"), None, "another source");
    }
}
