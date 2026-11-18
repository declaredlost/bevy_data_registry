//! Formats data files are written in, and what they read files into.
//!
//! [`DataFormat`] is a format; [`Ron`] the one that comes with the crate.
//! A format reads a file with a [`FileSeed`] into [`FileContents`], which
//! becomes a [`DataFile`] asset, or fails with a [`DataFileError`].

use bevy::prelude::*;

pub use crate::file::{DataFile, DataFileError, FileContents, FileSeed};

/// A format data files are written in, chosen by their extension.
///
/// Added to [`DataRegistryPlugin::formats`](crate::DataRegistryPlugin::formats).
///
/// A format only builds its deserializer over a file's bytes and runs the
/// [`FileSeed`] it's given on it: which kind the file holds, and whether
/// it holds one or a list, the seed works out. Any serde format fits whose
/// deserializer supports `deserialize_any` (JSON, TOML, YAML; not Postcard
/// or Bincode), as the seed asks it what the file holds:
///
/// A TOML file is a table, so holds one definition: a list (`[[items]]`)
/// is a table holding an array, which a TOML format would have to present
/// as the file itself to read as a list.
///
/// ```rust,ignore
/// use serde::de::DeserializeSeed;
///
/// struct Json;
///
/// impl DataFormat for Json {
///     fn extensions(&self) -> &[&'static str] {
///         &["json"]
///     }
///
///     fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError> {
///         let mut json = serde_json::Deserializer::from_slice(bytes);
///         let contents = seed.deserialize(&mut json)?;
///         json.end()?; // nothing after it
///         Ok(contents)
///     }
/// }
/// ```
pub trait DataFormat: Send + Sync + 'static {
    /// The extensions of its files, without the dot: `["ron"]`. Every file
    /// in the app ending so is read as data, wherever it is: another asset
    /// of this format needs another extension (`level.ron`).
    fn extensions(&self) -> &[&'static str];

    /// Reads `bytes`, a whole file, with `seed`.
    ///
    /// # Errors
    ///
    /// What the format's deserializer says: the file isn't valid in it, or
    /// isn't of its kind.
    fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError>;
}

/// The default format, [RON](https://github.com/ron-rs/ron):
/// `(id: "sword", price: 3)`.
#[derive(Clone)]
pub struct Ron {
    /// The extensions of its files: `ron` by default.
    pub extensions: Vec<&'static str>,
    /// How its files are read: RON's defaults by default.
    pub options: ron::Options,
}

impl Default for Ron {
    fn default() -> Self {
        Self {
            extensions: vec!["ron"],
            options: ron::Options::default(),
        }
    }
}

impl DataFormat for Ron {
    fn extensions(&self) -> &[&'static str] {
        &self.extensions
    }

    fn read(&self, bytes: &[u8], seed: FileSeed<'_>) -> Result<FileContents, BevyError> {
        Ok(self.options.from_bytes_seed(bytes, seed)?)
    }
}
