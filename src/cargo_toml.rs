//! This crate defines `struct`s that can be deserialized with Serde
//! to load and inspect `Cargo.toml` metadata.
//!
//! See `Manifest::from_slice`.

use std::fmt::Display;
use std::fs;
use std::io;
use std::path::Path;

#[macro_use]
extern crate serde_derive;
use serde::Deserialize;
use serde::Deserializer;
use std::collections::BTreeMap;
pub use toml::Value;

pub type DepsSet = BTreeMap<String, Dependency>;
pub type TargetDepsSet = BTreeMap<String, Target>;
pub type FeatureSet = BTreeMap<String, Vec<String>>;
pub type PatchSet = BTreeMap<String, DepsSet>;

mod afs;
mod error;
pub use crate::afs::*;
pub use crate::error::Error;

/// The top-level `Cargo.toml` structure
///
/// The `Metadata` is a type for `[package.metadata]` table. You can replace it with
/// your own struct type if you use the metadata and don't want to use the catch-all `Value` type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Manifest<Metadata = Value> {
    pub package: Option<Package<Metadata>>,
    pub workspace: Option<Workspace<Metadata>>,
    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "DepsSet::is_empty"
    )]
    pub dependencies: DepsSet,
    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "DepsSet::is_empty"
    )]
    pub dev_dependencies: DepsSet,
    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "DepsSet::is_empty"
    )]
    pub build_dependencies: DepsSet,
    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "TargetDepsSet::is_empty"
    )]
    pub target: TargetDepsSet,
    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "FeatureSet::is_empty"
    )]
    pub features: FeatureSet,

    #[serde(
        default,
        serialize_with = "toml::ser::tables_last",
        skip_serializing_if = "PatchSet::is_empty"
    )]
    pub patch: PatchSet,

    /// Note that due to autolibs feature this is not the complete list
    /// unless you run `complete_from_path`
    pub lib: Option<Product>,
    #[serde(default, skip_serializing_if = "Profiles::should_skip_serializing")]
    pub profile: Profiles,

    #[serde(default, skip_serializing_if = "Badges::should_skip_serializing")]
    pub badges: Badges,

    /// Note that due to autobins feature this is not the complete list
    /// unless you run `complete_from_path`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<Product>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bench: Vec<Product>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<Product>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub example: Vec<Product>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Workspace<Metadata = Value> {
    #[serde(default)]
    pub members: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_members: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,

    pub metadata: Option<Metadata>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<Resolver>,
}

fn default_true() -> bool {
    true
}

fn is_true(val: &bool) -> bool {
    *val
}

impl Manifest<Value> {
    /// Parse contents of a `Cargo.toml` file already loaded as a byte slice.
    ///
    /// It does not call `complete_from_path`, so may be missing implicit data.
    #[inline(always)]
    pub fn from_slice(cargo_toml_content: &[u8]) -> Result<Self, Error> {
        Self::from_slice_with_metadata(cargo_toml_content)
    }

    /// Parse contents from a `Cargo.toml` file on disk.
    ///
    /// Calls `complete_from_path`.
    #[inline]
    pub fn from_path(cargo_toml_path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_path_with_metadata(cargo_toml_path)
    }

    /// Parse contents of a `Cargo.toml` file loaded as a string
    ///
    /// Note: this is **not** a file name, but file's content. See `from_path`.
    ///
    /// It does not call `complete_from_path`, so may be missing implicit data.
    #[inline(always)]
    pub fn from_str(cargo_toml_content: &str) -> Result<Self, Error> {
        Self::from_slice_with_metadata(cargo_toml_content.as_bytes())
    }
}

impl<Metadata: for<'a> Deserialize<'a>> Manifest<Metadata> {
    /// Parse `Cargo.toml`, and parse its `[package.metadata]` into a custom Serde-compatible type.package
    ///
    /// It does not call `complete_from_path`, so may be missing implicit data.
    pub fn from_slice_with_metadata(cargo_toml_content: &[u8]) -> Result<Self, Error> {
        let mut manifest: Self = toml::from_slice(cargo_toml_content)?;
        if manifest.package.is_none() && manifest.workspace.is_none() {
            // Some old crates lack the `[package]` header

            let val: Value = toml::from_slice(cargo_toml_content)?;
            if let Some(project) = val.get("project") {
                manifest.package = Some(project.clone().try_into()?);
            } else {
                manifest.package = Some(val.try_into()?);
            }
        }
        Ok(manifest)
    }

    /// Parse contents from `Cargo.toml` file on disk, with custom Serde-compatible metadata type.
    ///
    /// Calls `complete_from_path`
    pub fn from_path_with_metadata(cargo_toml_path: impl AsRef<Path>) -> Result<Self, Error> {
        let cargo_toml_path = cargo_toml_path.as_ref();
        let cargo_toml_content = fs::read(cargo_toml_path)?;
        let mut manifest = Self::from_slice_with_metadata(&cargo_toml_content)?;
        manifest.complete_from_path(cargo_toml_path)?;
        Ok(manifest)
    }

    /// `Cargo.toml` doesn't contain explicit information about `[lib]` and `[[bin]]`,
    /// which are inferred based on files on disk.
    ///
    /// This scans the disk to make the data in the manifest as complete as possible.
    pub fn complete_from_path(&mut self, path: &Path) -> Result<(), Error> {
        let manifest_dir = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "bad path"))?;
        self.complete_from_abstract_filesystem(Filesystem::new(manifest_dir))
    }

    /// `Cargo.toml` doesn't contain explicit information about `[lib]` and `[[bin]]`,
    /// which are inferred based on files on disk.
    ///
    /// You can provide any implementation of directory scan, which doesn't have to
    /// be reading straight from disk (might scan a tarball or a git repo, for example).
    pub fn complete_from_abstract_filesystem(
        &mut self,
        fs: impl AbstractFilesystem,
    ) -> Result<(), Error> {
        if let Some(ref package) = self.package {
            let src = match fs.file_names_in("src") {
                Ok(src) => src,
                Err(err) if err.kind() == io::ErrorKind::NotFound => Default::default(),
                Err(err) => return Err(err.into()),
            };

            if let Some(ref mut lib) = self.lib {
                lib.required_features.clear(); // not applicable
            }

            let has_path = self.lib.as_ref().map_or(false, |l| l.path.is_some());
            if !has_path && src.contains("lib.rs") {
                let old_lib = self.lib.take().unwrap_or_default();
                self.lib = Some(Product {
                    name: Some(package.name.replace("-", "_")),
                    path: Some("src/lib.rs".to_string()),
                    edition: Some(package.edition),
                    crate_type: Some(vec!["rlib".to_string()]),
                    ..old_lib
                })
            }

            if package.autobins && self.bin.is_empty() {
                self.bin = self.autoset("src/bin", &fs);
                if src.contains("main.rs") {
                    self.bin.push(Product {
                        name: Some(package.name.clone()),
                        path: Some("src/main.rs".to_string()),
                        edition: Some(package.edition),
                        ..Product::default()
                    })
                }
            }
            if package.autoexamples && self.example.is_empty() {
                self.example = self.autoset("examples", &fs);
            }
            if package.autotests && self.test.is_empty() {
                self.test = self.autoset("tests", &fs);
            }
            if package.autobenches && self.bench.is_empty() {
                self.bench = self.autoset("benches", &fs);
            }
        }
        if let Some(ref mut package) = self.package {
            if matches!(package.build, None | Some(OptionalFile::Flag(true))) {
                if fs.file_names_in(".").map_or(false, |dir| dir.contains("build.rs")) {
                    package.build = Some(OptionalFile::Path("build.rs".into()));
                }
            }
            if matches!(package.readme, OptionalFile::Flag(true)) {
                let files = fs.file_names_in(".").ok();
                if let Some(name) = files.as_ref().and_then(|dir| dir.get("README.md").or_else(|| dir.get("README.txt")).or_else(|| dir.get("README"))) {
                    package.build = Some(OptionalFile::Path((**name).to_owned()));
                }
            }
        }
        Ok(())
    }

    fn autoset(&self, dir: &str, fs: &dyn AbstractFilesystem) -> Vec<Product> {
        let mut out = Vec::new();
        if let Some(ref package) = self.package {
            if let Ok(bins) = fs.file_names_in(dir) {
                for name in bins {
                    let rel_path = format!("{}/{}", dir, name);
                    if name.ends_with(".rs") {
                        out.push(Product {
                            name: Some(name.trim_end_matches(".rs").into()),
                            path: Some(rel_path),
                            edition: Some(package.edition),
                            ..Product::default()
                        })
                    } else if let Ok(sub) = fs.file_names_in(&rel_path) {
                        if sub.contains("main.rs") {
                            out.push(Product {
                                name: Some(name.into()),
                                path: Some(rel_path + "/main.rs"),
                                edition: Some(package.edition),
                                ..Product::default()
                            })
                        }
                    }
                }
            }
        }
        // ensure bins are deterministic
        out.sort_by(|a,b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
        out
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Profiles {
    pub release: Option<Profile>,
    pub dev: Option<Profile>,
    pub test: Option<Profile>,
    pub bench: Option<Profile>,
    pub doc: Option<Profile>,
}

impl Profiles {
    /// Determine whether or not a Profiles struct should be serialized
    fn should_skip_serializing(&self) -> bool {
        self.release.is_none()
            && self.dev.is_none()
            && self.test.is_none()
            && self.bench.is_none()
            && self.doc.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Profile {
    /// num or z, s
    pub opt_level: Option<Value>,
    /// 0,1,2 or bool
    pub debug: Option<Value>,
    pub rpath: Option<bool>,
    pub lto: Option<Value>,
    pub debug_assertions: Option<bool>,
    pub codegen_units: Option<u16>,
    pub panic: Option<String>,
    pub incremental: Option<bool>,
    pub overflow_checks: Option<bool>,
    pub strip: Option<bool>,

    /// profile overrides
    #[serde(default, serialize_with = "toml::ser::tables_last")]
    pub package: BTreeMap<String, Value>,
    /// profile overrides
    pub build_override: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Cargo uses the term "target" for both "target platform" and "build target" (the thing to build),
/// which makes it ambigous.
/// Here Cargo's bin/lib **target** is renamed to **product**.
pub struct Product {
    /// This field points at where the crate is located, relative to the `Cargo.toml`.
    pub path: Option<String>,

    /// The name of a product is the name of the library or binary that will be generated.
    /// This is defaulted to the name of the package, with any dashes replaced
    /// with underscores. (Rust `extern crate` declarations reference this name;
    /// therefore the value must be a valid Rust identifier to be usable.)
    pub name: Option<String>,

    /// A flag for enabling unit tests for this product. This is used by `cargo test`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub test: bool,

    /// A flag for enabling documentation tests for this product. This is only relevant
    /// for libraries, it has no effect on other sections. This is used by
    /// `cargo test`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub doctest: bool,

    /// A flag for enabling benchmarks for this product. This is used by `cargo bench`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub bench: bool,

    /// A flag for enabling documentation of this product. This is used by `cargo doc`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub doc: bool,

    /// If the product is meant to be a compiler plugin, this field must be set to true
    /// for Cargo to correctly compile it and make it available for all dependencies.
    #[serde(default)]
    pub plugin: bool,

    /// If the product is meant to be a "macros 1.1" procedural macro, this field must
    /// be set to true.
    #[serde(default, alias = "proc_macro", alias = "proc-macro")]
    pub proc_macro: bool,

    /// If set to false, `cargo test` will omit the `--test` flag to rustc, which
    /// stops it from generating a test harness. This is useful when the binary being
    /// built manages the test runner itself.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub harness: bool,

    /// If set then a product can be configured to use a different edition than the
    /// `[package]` is configured to use, perhaps only compiling a library with the
    /// 2018 edition or only compiling one unit test with the 2015 edition. By default
    /// all products are compiled with the edition specified in `[package]`.
    #[serde(default)]
    pub edition: Option<Edition>,

    /// The available options are "dylib", "rlib", "staticlib", "cdylib", and "proc-macro".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_type: Option<Vec<String>>,

    /// The required-features field specifies which features the product needs in order to be built.
    /// If any of the required features are not selected, the product will be skipped.
    /// This is only relevant for the `[[bin]]`, `[[bench]]`, `[[test]]`, and `[[example]]` sections,
    /// it has no effect on `[lib]`.
    #[serde(default)]
    pub required_features: Vec<String>,
}

impl Default for Product {
    fn default() -> Self {
        Self {
            path: None,
            name: None,
            test: true,
            doctest: true,
            bench: true,
            doc: true,
            harness: true,
            plugin: false,
            proc_macro: false,
            required_features: Vec::new(),
            crate_type: None,
            edition: Some(Edition::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Target {
    #[serde(default, serialize_with = "toml::ser::tables_last")]
    pub dependencies: DepsSet,
    #[serde(default, serialize_with = "toml::ser::tables_last")]
    pub dev_dependencies: DepsSet,
    #[serde(default, serialize_with = "toml::ser::tables_last")]
    pub build_dependencies: DepsSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Detailed(DependencyDetail),
}

impl Dependency {
    #[inline]
    pub fn detail(&self) -> Option<&DependencyDetail> {
        match *self {
            Dependency::Simple(_) => None,
            Dependency::Detailed(ref d) => Some(d),
        }
    }

    #[inline]
    pub fn req(&self) -> &str {
        match *self {
            Dependency::Simple(ref v) => v,
            Dependency::Detailed(ref d) => d.version.as_deref().unwrap_or("*"),
        }
    }

    #[inline]
    pub fn req_features(&self) -> &[String] {
        match *self {
            Dependency::Simple(_) => &[],
            Dependency::Detailed(ref d) => &d.features,
        }
    }

    #[inline]
    pub fn optional(&self) -> bool {
        self.detail().map_or(false, |d| d.optional)
    }

    // `Some` if it overrides the package name.
    // If `None`, use the dependency name as the package name.
    #[inline]
    pub fn package(&self) -> Option<&str> {
        match *self {
            Dependency::Simple(_) => None,
            Dependency::Detailed(ref d) => d.package.as_deref(),
        }
    }

    // Git URL of this dependency, if any
    #[inline]
    pub fn git(&self) -> Option<&str> {
        self.detail().and_then(|d| d.git.as_deref())
    }

    // `true` if it's an usual crates.io dependency,
    // `false` if git/path/alternative registry
    pub fn is_crates_io(&self) -> bool {
        match *self {
            Dependency::Simple(_) => true,
            Dependency::Detailed(ref d) => {
                // TODO: allow registry to be set to crates.io explicitly?
                d.path.is_none()
                    && d.registry.is_none()
                    && d.registry_index.is_none()
                    && d.git.is_none()
                    && d.tag.is_none()
                    && d.branch.is_none()
                    && d.rev.is_none()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DependencyDetail {
    pub version: Option<String>,
    pub registry: Option<String>,
    pub registry_index: Option<String>,
    pub path: Option<String>,
    pub git: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub rev: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    pub default_features: Option<bool>,
    pub package: Option<String>,
}

/// You can replace `Metadata` type with your own
/// to parse into something more useful than a generic toml `Value`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Package<Metadata = Value> {
    /// Careful: some names are uppercase
    pub name: String,
    #[serde(default)]
    pub edition: Edition,
    /// MSRV 1.x (beware: does not require semver formatting)
    #[serde(default)]
    pub rust_version: Option<String>,
    /// e.g. "1.9.0"
    pub version: String,
    #[serde(default)]
    pub build: Option<OptionalFile>,
    pub workspace: Option<String>,
    #[serde(default)]
    /// e.g. ["Author <e@mail>", "etc"] Deprecated.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    pub links: Option<String>,
    /// A short blurb about the package. This is not rendered in any format when
    /// uploaded to crates.io (aka this is not markdown).
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    /// This points to a file under the package root (relative to this `Cargo.toml`).
    /// implied if README.md, README.txt or README exists.
    #[serde(default, skip_serializing_if = "OptionalFile::is_default")]
    pub readme: OptionalFile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// This is a list of up to five categories where this crate would fit.
    /// e.g. ["command-line-utilities", "development-tools::cargo-plugins"]
    pub categories: Vec<String>,
    /// e.g. "MIT"
    pub license: Option<String>,
    pub license_file: Option<String>,
    pub repository: Option<String>,

    /// The default binary to run by cargo run.
    pub default_run: Option<String>,

    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub autobins: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub autoexamples: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub autotests: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub autobenches: bool,
    #[serde(default, skip_serializing_if = "Publish::is_default")]
    pub publish: Publish,

    /// "2" is the only useful value
    pub resolver: Option<Resolver>,

    pub metadata: Option<Metadata>,
}

impl<T> Package<T> {
    /// Prefer creating it by parsing `Manifest` instead
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            edition: Edition::E2021,
            rust_version: None,
            build: None,
            workspace: None,
            authors: Default::default(),
            links: None,
            description: None,
            homepage: None,
            documentation: None,
            readme: OptionalFile::Flag(true),
            keywords: Default::default(),
            categories: Default::default(),
            license: None,
            license_file: None,
            repository: None,
            default_run: None,
            autobins: true,
            autoexamples: true,
            autotests: true,
            autobenches: true,
            publish: Publish::Flag(true),
            resolver: None,
            metadata: None,
        }
    }
}

/// Readme of build.rs path
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionalFile {
    /// Opt-in to default, or explicit opt-out
    Flag(bool),
    /// Explicit path
    Path(String),
}

impl Default for OptionalFile {
    #[inline]
    fn default() -> Self {
        Self::Flag(true)
    }
}

impl OptionalFile {
    #[inline]
    fn is_default(&self) -> bool {
        matches!(self, Self::Flag(flag) if *flag)
    }

    #[inline]
    pub fn as_ref(&self) -> Option<&str> {
        match self {
            Self::Path(p) => Some(p),
            _ => None,
        }
    }

    #[inline]
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Flag(true) | Self::Path(_))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Publish {
    Flag(bool),
    Registry(Vec<String>),
}

impl Publish {
    fn is_default(&self) -> bool {
        matches!(self, Publish::Flag(flag) if *flag)
    }
}

impl Default for Publish {
    #[inline]
    fn default() -> Self {
        Publish::Flag(true)
    }
}

impl PartialEq<Publish> for bool {
    #[inline]
    fn eq(&self, p: &Publish) -> bool {
        match *p {
            Publish::Flag(flag) => flag == *self,
            Publish::Registry(ref reg) => reg.is_empty() != *self,
        }
    }
}

impl PartialEq<bool> for Publish {
    #[inline]
    fn eq(&self, b: &bool) -> bool {
        b.eq(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Badge {
    pub repository: String,
    #[serde(default = "default_master")]
    pub branch: String,
    pub service: Option<String>,
    pub id: Option<String>,
    pub project_name: Option<String>,
}

fn default_master() -> String {
    "master".to_string()
}

fn ok_or_default<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: Deserializer<'de>,
{
    Ok(Deserialize::deserialize(deserializer).unwrap_or_default())
}

/// Deprecated
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Badges {
    /// Appveyor: `repository` is required. `branch` is optional; default is `master`
    /// `service` is optional; valid values are `github` (default), `bitbucket`, and
    /// `gitlab`; `id` is optional; you can specify the appveyor project id if you
    /// want to use that instead. `project_name` is optional; use when the repository
    /// name differs from the appveyor project name.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub appveyor: Option<Badge>,

    /// Circle CI: `repository` is required. `branch` is optional; default is `master`
    #[serde(default, deserialize_with = "ok_or_default")]
    pub circle_ci: Option<Badge>,

    /// GitLab: `repository` is required. `branch` is optional; default is `master`
    #[serde(default, deserialize_with = "ok_or_default")]
    pub gitlab: Option<Badge>,

    /// Travis CI: `repository` in format "<user>/<project>" is required.
    /// `branch` is optional; default is `master`
    #[serde(default, deserialize_with = "ok_or_default")]
    pub travis_ci: Option<Badge>,

    /// Codecov: `repository` is required. `branch` is optional; default is `master`
    /// `service` is optional; valid values are `github` (default), `bitbucket`, and
    /// `gitlab`.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub codecov: Option<Badge>,

    /// Coveralls: `repository` is required. `branch` is optional; default is `master`
    /// `service` is optional; valid values are `github` (default) and `bitbucket`.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub coveralls: Option<Badge>,

    /// Is it maintained resolution time: `repository` is required.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub is_it_maintained_issue_resolution: Option<Badge>,

    /// Is it maintained percentage of open issues: `repository` is required.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub is_it_maintained_open_issues: Option<Badge>,

    /// Maintenance: `status` is required. Available options are `actively-developed`,
    /// `passively-maintained`, `as-is`, `experimental`, `looking-for-maintainer`,
    /// `deprecated`, and the default `none`, which displays no badge on crates.io.
    #[serde(default, deserialize_with = "ok_or_default")]
    pub maintenance: Maintenance,
}

impl Badges {
    /// Determine whether or not a Profiles struct should be serialized
    fn should_skip_serializing(&self) -> bool {
        self.appveyor.is_none()
            && self.circle_ci.is_none()
            && self.gitlab.is_none()
            && self.travis_ci.is_none()
            && self.codecov.is_none()
            && self.coveralls.is_none()
            && self.is_it_maintained_issue_resolution.is_none()
            && self.is_it_maintained_open_issues.is_none()
            && matches!(self.maintenance.status, MaintenanceStatus::None)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Default, Serialize, Deserialize)]
pub struct Maintenance {
    pub status: MaintenanceStatus,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaintenanceStatus {
    None,
    ActivelyDeveloped,
    PassivelyMaintained,
    AsIs,
    Experimental,
    LookingForMaintainer,
    Deprecated,
}

impl Default for MaintenanceStatus {
    fn default() -> Self {
        MaintenanceStatus::None
    }
}

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub enum Edition {
    #[serde(rename = "2015")]
    E2015 = 2015,
    #[serde(rename = "2018")]
    E2018 = 2018,
    #[serde(rename = "2021")]
    E2021 = 2021,
}

impl Default for Edition {
    fn default() -> Self {
        Self::E2015
    }
}

#[derive(Debug, PartialEq, Eq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub enum Resolver {
    #[serde(rename = "1")]
    V1 = 1,
    #[serde(rename = "2")]
    V2 = 2,
}

impl Display for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Resolver::V1 => "1",
                Resolver::V2 => "2",
            }
        )
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::V1
    }
}
