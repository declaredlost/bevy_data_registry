# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `bevy_data_registry`: data definitions, indexed by id, with families,
  validation across kinds and hot reload. Moved from the abyss repository.
- `bevy_data_registry_macros`: `#[derive(Definition)]`, setting which data
  files a kind claims, its id, base and family flag; and
  `#[definition(crate = "..")]` where `bevy_data_registry` is renamed or
  re-exported.
- `Resolve`, a rule for which files are a kind's, with `FileName`,
  `Folder` and `Extension`; set with `#[definition(file = "..")]`,
  `folder`, `extension` or `resolve = ..`.
- `DataFormat`, a format data files are written in, chosen by extension;
  `Ron` built in, any serde format added in a few lines.
- Examples: `basic`, `families`, `validation` and `hot_reload`.

### Changed

- Bevy 0.20 and RON 0.12.
- Which kind a file holds follows from the one kind whose rules claim it,
  each kind declaring its own: a file no kind claims, or two kinds do, is
  an error rather than settled by precedence (was `kind_of`: the first
  kind folder on the path, else the file's name). The data folder is
  loaded with Bevy's `load_folder`, which, with the `dev` feature (Bevy's
  file watcher), also hot-reloads files added and removed.
- Bevy-style API: `DataRegistryPlugin` configured by its fields (`path`,
  `formats`; `..default()`), `app.register_definition::<T>()` for every
  kind, `Definitions<T>` (was `Defs<T>`) looked up by borrowed ids, a
  `prelude`. `FamilyDefinition` became `Inherit`, with the base and family
  fields marked for the derive.

### Removed

- `kind_of`, `Definition::FOLDER` (the derive's rules say which files),
  `add_family_definitions`, `Registry`'s and `Location`'s public API,
  `resolve`, `duplicates` and `DataAsset`; and the `bevy_common_assets`
  dependency.
