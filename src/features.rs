//! Helper for parsing syntax of the `[features]` section

use crate::{Dependency, Manifest, Product, DepsSet, TargetDepsSet};
use std::collections::hash_map::{Entry, RandomState};
use std::collections::{HashMap, BTreeMap, BTreeSet};
use std::hash::BuildHasher;
use std::marker::PhantomData;

/// Call [`features::Resolver::new()`](Resolver::new) to get started.
///
/// The extra `Hasher` arg is for optionally using [`ahash`](https://lib.rs/ahash).
pub struct Resolver<'config, Hasher = RandomState> {
    always_keep: Option<&'config dyn Fn(&str) -> bool>,
    _hasher: PhantomData<fn() -> Hasher>,
}

/// Parse result
///
/// It's a temporary struct that borrows from the manifest. Copy things out of this if lifetimes get in the way.
///
/// The extra `Hasher` arg is for optionally using [`ahash`](https://lib.rs/ahash).
pub struct Features<'manifest, 'deps, Hasher = RandomState> {
    /// All features, resolved and normalized
    pub features: HashMap<&'manifest str, Feature<'manifest>, Hasher>,
    /// Dependencies referenced by the features
    pub dependencies: HashMap<&'deps str, FeatureDependency<'deps>, Hasher>,
    /// True if there were features with names staring with `_` and were merged into other features
    ///
    /// See arg of [`new_with_hasher_and_filter`](Resolver::new_with_hasher_and_filter) to disable removal.
    pub removed_hidden_features: bool,
}

/// How an enabled feature affects the dependency
#[derive(Debug, PartialEq)]
pub struct DepAction<'a> {
    /// Uses `?` syntax, so it doesn't enable the depenency
    pub is_conditional: bool,
    /// Uses `dep:` or `?` syntax, so it doesn't imply a feature name
    pub is_dep_only: bool,
    /// Features of the dependency to enable (the text after slash, possibly aggregated from multiple items)
    pub dep_features: Vec<&'a str>,
}

/// A feature from `[features]` with all the details
#[derive(Debug)]
pub struct Feature<'a> {
    /// Name of the feature
    pub key: &'a str,
    /// Deps by their manifest key (the key isn't always the same as the crate name)
    pub enables_deps: BTreeMap<&'a str, DepAction<'a>>,
    /// Enables these explicitly named features
    pub enables_features: BTreeSet<&'a str>,
    /// Keys of other features that enable this feature (this is shallow, not recursive)
    pub enabled_by: BTreeSet<&'a str>,

    /// Names of binaries that have `required-features = [this feature]`
    pub required_by_bins: Vec<&'a str>,

    /// If true, it's from `[features]`. If false, it's from `[dependencies]`.
    pub explicit: bool,
}

impl<'a> Feature<'a> {
    /// `enabled_by` except `"default"`
    #[inline]
    #[must_use]
    pub fn non_default_enabled_by(&self) -> impl Iterator<Item = &str> {
        self.enabled_by.iter().copied().filter(|&e| e != "default")
    }

    /// Is any feature using this one?
    #[inline]
    #[must_use]
    pub fn is_referenced(&self) -> bool {
        !self.enabled_by.is_empty()
    }
}

/// Extra info for dependency referenced by a feature
pub struct FeatureDependencyDetail<'dep> {
    /// Features may refer to non-optional dependencies, only enable *their* features
    pub is_optional: bool,
    /// If it's enabled by default, other targets are ignored and this is empty
    pub only_for_targets: BTreeSet<&'dep str>,
    /// Details about this dependency
    pub dep: &'dep Dependency,
}

/// A dependency referenced by a feature
pub struct FeatureDependency<'dep> {
    /// Actual crate of this dependency. Note that multiple dependencies can be the same crate, in different versions.
    pub crate_name: &'dep str,

    /// At least one of these will be set
    pub normal: Option<FeatureDependencyDetail<'dep>>,
    pub build: Option<FeatureDependencyDetail<'dep>>,
    pub dev: Option<FeatureDependencyDetail<'dep>>,
}

impl<'dep> FeatureDependency<'dep> {
    #[inline]
    #[must_use]
    pub fn dep(&self) -> &'dep Dependency {
        self.detail().0.dep
    }

    #[inline]
    #[must_use]
    pub fn detail(&self) -> (&FeatureDependencyDetail<'dep>, Kind) {
        [
            (self.normal.as_ref(), Kind::Normal),
            (self.build.as_ref(), Kind::Build),
            (self.dev.as_ref(), Kind::Dev)
        ].into_iter()
        .find_map(|(detail, kind)| Some((detail?, kind)))
        .unwrap()
    }

    #[inline]
    #[must_use]
    fn get_mut_entry(&mut self, kind: Kind) -> &mut Option<FeatureDependencyDetail<'dep>> {
        match kind {
            Kind::Normal => &mut self.normal,
            Kind::Build => &mut self.build,
            Kind::Dev => &mut self.dev,
        }
    }
}

impl Resolver<'static, RandomState> {
    /// Next step: [`.parse(manifest)`](Resolver::parse).
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            always_keep: None,
            _hasher: PhantomData,
        }
    }
}

impl<'manifest, 'config, RandomState: BuildHasher + Default> Resolver<'config, RandomState> {
    /// Use turbofish to configure `RandomState` of a hasher you want this to use.
    ///
    /// `should_keep_hidden_feature` is a reference to a closure that will receive feature names starting with `_`, and return whether to keep or hide them.
    #[must_use]
    pub fn new_with_hasher_and_filter(should_keep_hidden_feature: &'config dyn Fn(&str) -> bool) -> Self {
        Self {
            always_keep: Some(should_keep_hidden_feature),
            _hasher: PhantomData,
        }
    }

    /// Parse features from a Cargo.toml manifest
    pub fn parse<M>(&self, manifest: &'manifest Manifest<M>) -> Features<'manifest, 'manifest, RandomState> {
        let mut features = Self::parse_features(
            manifest.features.iter().take(1000),
            manifest.features.contains_key("default"),
        );

        let dependencies = Self::add_dependencies(
            &mut features,
            &manifest.dependencies,
            &manifest.build_dependencies,
            &manifest.dev_dependencies,
            &manifest.target,
        );

        Self::set_required_by_bins(&mut features, &manifest.bin, manifest.package().name());

        Self::set_enabled_by(&mut features);
        let removed_hidden_features = self.remove_hidden_features(&mut features);

        Features {
            features,
            dependencies,
            removed_hidden_features,
        }
    }

    /// Instead of processing a `Cargo.toml` manifest, take bits of information directly instead
    ///
    /// It won't fill in `required_by_bins` fields.
    pub fn parse_custom<'deps, S: BuildHasher>(&self, manifest_features: &'manifest HashMap<String, Vec<String>, S>, deps: impl Iterator<Item=ParseDependency<'manifest, 'deps>>) -> Features<'manifest, 'deps, RandomState> where 'manifest: 'deps {
        let mut features: HashMap<&'manifest str, Feature<'manifest>, _> = Self::parse_features(
            manifest_features.iter().take(1000),
            manifest_features.contains_key("default"),
        );

        let named_using_dep_syntax = Self::named_using_dep_syntax(&features);

        // First one wins, so order is important
        let mut dependencies = HashMap::<&'deps str, FeatureDependency<'deps>, RandomState>::default();
        for dep in deps {
            Self::add_dependency(&mut features, &mut dependencies, named_using_dep_syntax.get(dep.key).copied(), dep.kind, dep.target, dep.key, dep.dep);
        }

        Self::set_enabled_by(&mut features);
        let removed_hidden_features = self.remove_hidden_features(&mut features);

        Features {
            features,
            dependencies,
            removed_hidden_features,
        }
    }
}

/// For parsing in `parse_custom`. Can be constructed from the crates.io index, instead of `Cargo.toml`.
///
/// Note about lifetimes: it's not possible to make `&Dependency` on the fly.
/// You will have to collect *owned* `Dependency` objects to a `Vec` or `HashMap` first.
pub struct ParseDependency<'a, 'tmp> {
    /// Name/id of the dependency, not always the crate name
    pub key: &'a str,
    pub kind: Kind,
    /// Name `[target."from here".dependencies]`
    pub target: Option<&'a str>,
    /// Possibly more detail
    pub dep: &'tmp Dependency,
}

#[derive(Copy, Clone, PartialEq)]
pub enum Kind {
    Normal,
    Dev,
    Build,
}


impl<'a, 'c, S: BuildHasher + Default> Resolver<'c, S> {
    fn parse_features(features: impl Iterator<Item = (&'a String, &'a Vec<String>)>, has_explicit_default: bool) -> HashMap<&'a str, Feature<'a>, S> {
        features
            .map(|(key, f)| (key.as_str(), f.as_slice()))
            .chain(if has_explicit_default { None } else { Some(("default", &[][..])) }) // there must always be the default key
            .map(|(feature_key, actions)| Self::parse_feature(feature_key, actions))
            .collect()
    }

    #[inline(never)]
    fn parse_feature(feature_key: &'a str, actions: &'a [String]) -> (&'a str, Feature<'a>) {
        // coalesce dep_feature
        let mut enables_deps = BTreeMap::new();
        let mut enables_features = BTreeSet::new();
        actions.iter().for_each(|action| {
            let mut parts = action.splitn(2, '/');
            let mut atarget = parts.next().unwrap_or_default();
            let dep_feature = parts.next();

            let is_dep_prefix = if let Some(k) = atarget.strip_prefix("dep:") { atarget = k; true } else { false };
            let is_conditional = if let Some(k) = atarget.strip_suffix('?') { atarget = k; true  } else { false };

            // dep-feature is old, doesn't actually mean it's dep:only!
            let is_dep_only = is_dep_prefix || is_conditional;
            if is_dep_only || dep_feature.is_some() {
                let action = enables_deps.entry(atarget).or_insert(DepAction {
                    is_conditional,
                    is_dep_only,
                    dep_features: vec![],
                });
                if !is_conditional { action.is_conditional = false; }
                if let Some(df) = dep_feature { action.dep_features.push(df); }
            } else {
                // dep/foo can be both
                enables_features.insert(atarget);
            }
        });

        (feature_key, Feature {
            key: feature_key,
            enables_features,
            required_by_bins: vec![],
            enables_deps,
            explicit: true,
            enabled_by: BTreeSet::new(), // fixed later
        })
    }

    fn add_implied_optional_deps(features: &mut HashMap<&'a str, Feature<'a>, S>, deps_for_features: &mut HashMap<&'a str, FeatureDependency<'a>, S>, crate_deps: &'a BTreeMap<String, Dependency>, named_using_dep_syntax: &HashMap<&str, bool, S>, dep_kind: Kind, only_for_target: Option<&'a str>) {
        for (key, dep) in crate_deps.iter().take(1000) {
            let key = key.as_str();
            Self::add_dependency(features, deps_for_features, named_using_dep_syntax.get(key).copied(), dep_kind, only_for_target, key, dep);
        }
    }

    #[inline(never)]
    fn add_dependency<'d>(features: &mut HashMap<&'a str, Feature<'a>, S>, deps_for_features: &mut HashMap<&'d str, FeatureDependency<'d>, S>, named_using_dep_syntax: Option<bool>, dep_kind: Kind, only_for_target: Option<&'d str>, key: &'a str, dep: &'d Dependency) where 'a: 'd {
        let is_optional = dep.optional();
        let entry = deps_for_features.entry(key);
        if !is_optional && named_using_dep_syntax.is_none() && matches!(entry, Entry::Vacant(_)) {
            return;
        }
        let entry = entry.or_insert_with(move || FeatureDependency {
            crate_name: dep.package().unwrap_or(key),
            normal: None,
            build: None,
            dev: None,
        });
        match entry.get_mut_entry(dep_kind) {
            Some(out) => {
                // target-specific non-optional doesn't affect optionality for other targets
                if let Some(target) = only_for_target {
                    // if the dep is already used for all targets, then the target-specific details won't change that
                    if !out.only_for_targets.is_empty() {
                        out.only_for_targets.insert(target);
                    }
                } else {
                    out.only_for_targets.clear();
                    if !is_optional && out.is_optional {
                        out.is_optional = false;
                        out.dep = dep;
                    }
                }
            },
            out @ None => {
                *out = Some(FeatureDependencyDetail {
                    dep,
                    is_optional,
                    // if creating, this is the first time seeing the dep, so it is target-specific, since general deps were processed earlier
                    only_for_targets: only_for_target.into_iter().collect(),
                });
            }
        };

        // explicit features never overlap with deps, unless using "dep:" syntax.

        // We need to know about all deps referenced by features, or all optional ones.
        // We won't see optional build or dev dep feature, if there's normal non-optional dep. Not a big deal?
        // if we added one for normal, then keep adding build and dev.
        if is_optional && named_using_dep_syntax != Some(true) {
            features.entry(key).or_insert_with(|| Feature {
                key,
                enables_features: BTreeSet::default(),
                enables_deps: BTreeMap::from_iter([(key, DepAction {
                    is_dep_only: false,
                    is_conditional: false,
                    dep_features: vec![],
                })]),
                explicit: false,
                enabled_by: BTreeSet::new(), // will do later
                required_by_bins: vec![],
            });
        }
    }

    /// find which names are affected by use of the `dep:` syntax and supress implicit features
    #[inline(never)]
    fn named_using_dep_syntax(features: &HashMap<&'a str, Feature<'a>, S>) -> HashMap<&'a str, bool, S> {
        // explicit features exist, even if their name clashes with a `dep:name`.
        let mut named_using_dep_syntax: HashMap::<_, _, S> = features.keys().map(|&k| (k, false)).collect();

        features.values().for_each(|f| {
            f.enables_deps.iter().for_each(|(&dep_key, a)| {
                named_using_dep_syntax.entry(dep_key).or_insert(a.is_dep_only);
            });
        });

        named_using_dep_syntax
    }

    fn add_dependencies(features: &mut HashMap<&'a str, Feature<'a>, S>, dependencies: &'a DepsSet, build_dependencies: &'a DepsSet, dev_dependencies: &'a DepsSet, target: &'a TargetDepsSet) -> HashMap<&'a str, FeatureDependency<'a>, S> {
        let named_using_dep_syntax = Self::named_using_dep_syntax(features);

        // First one wins, so order is important
        let mut all_deps = HashMap::<_, _, S>::default();
        Self::add_implied_optional_deps(features, &mut all_deps, dependencies, &named_using_dep_syntax, Kind::Normal, None);
        Self::add_implied_optional_deps(features, &mut all_deps, build_dependencies, &named_using_dep_syntax, Kind::Build, None);
        for (target_cfg, target_deps) in target  {
            Self::add_implied_optional_deps(features, &mut all_deps, &target_deps.dependencies, &named_using_dep_syntax, Kind::Normal, Some(target_cfg));
            Self::add_implied_optional_deps(features, &mut all_deps, &target_deps.build_dependencies, &named_using_dep_syntax, Kind::Build, Some(target_cfg));
        }
        Self::add_implied_optional_deps(features, &mut all_deps, dev_dependencies, &named_using_dep_syntax, Kind::Dev, None);
        for (target_cfg, target_deps) in target  {
            Self::add_implied_optional_deps(features, &mut all_deps, &target_deps.dev_dependencies, &named_using_dep_syntax, Kind::Dev, Some(target_cfg));
        }
        all_deps
    }

    #[inline(never)]
    fn set_required_by_bins(features: &mut HashMap<&'a str, Feature<'a>, S>, bin: &'a [Product], package_name: &'a str) {
        bin.iter().for_each(|bin| {
            for f in &bin.required_features {
                let bin_name = bin.name.as_deref().unwrap_or(package_name);
                if let Some(f) = features.get_mut(f.as_str()) {
                    // fallback to package name is not quite accurate, because Cargo normalizes exe names slightly
                    f.required_by_bins.push(bin_name);
                }
            }
        });
    }

    #[inline(never)]
    fn set_enabled_by(features: &mut HashMap<&'a str, Feature<'a>, S>) {
        let mut all_enabled_by = HashMap::<_, _, S>::default();
        features.iter().for_each(|(&feature_key, f)| {
            f.enables_features.iter().copied()
                .for_each(|action_key| {
                    all_enabled_by.entry(action_key).or_insert_with(BTreeSet::new).insert(feature_key);
                });
        });

        // TODO: resolve features recursively? ["enables_foo", "foo?/x"] may happen
        all_enabled_by.into_iter().for_each(|(key, enabled_by)| {
            if let Some(f) = features.get_mut(key) {
                f.enabled_by = enabled_by;
            }
        });
    }

    /// find `__features`
    #[inline(never)]
    fn remove_hidden_features(&self, features: &mut HashMap<&str, Feature<'_>, S>) -> bool {
        let mut janky_features = HashMap::<_, _, S>::default();
        features.retain(|&k, f| {
            let remove = k.starts_with('_') &&
                f.enables_deps.is_empty() && // can't be bothered to merge actions properly
                !self.always_keep.map_or(false, |cb| (cb)(k)) && // if user thinks that is useful info
                !f.enables_features.iter().any(|k| k.starts_with('_')); // recursive bad features are too annoying to remove
            if remove {
                janky_features.insert(k, (std::mem::take(&mut f.enables_features), std::mem::take(&mut f.enabled_by))); false
            } else {
                true
            }
        });

        let removed_hidden_features = !janky_features.is_empty();

        // remove __features
        janky_features.into_iter().for_each(|(bad_feature, (bad_enables_features, bad_enabled_by))| {
            bad_enabled_by.iter().for_each(|&affected| {
                if let Some(f) = features.get_mut(affected) {
                    f.enables_features.remove(bad_feature);
                    f.enables_features.extend(&bad_enables_features);
                    f.enables_deps.remove(bad_feature);
                }
            });
            bad_enables_features.into_iter().for_each(|target| {
                if let Some(f) = features.get_mut(target) {
                    f.enabled_by.remove(bad_feature);
                    f.enabled_by.extend(&bad_enabled_by);
                }
            });
        });
        removed_hidden_features
    }
}
