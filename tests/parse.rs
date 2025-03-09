use cargo_toml::{Lint, LintLevel, Manifest, StripSetting};
use std::fs::read;
use std::path::Path;

#[test]
fn own() {
    let m = Manifest::from_slice(&read("Cargo.toml").unwrap()).unwrap();
    let package = m.package();
    assert_eq!("cargo_toml", package.name);
    let m = Manifest::<toml::Value>::from_slice_with_metadata(&read("Cargo.toml").unwrap()).unwrap();
    let package = m.package();
    assert_eq!("cargo_toml", package.name);
    assert_eq!(cargo_toml::Edition::E2021, package.edition());
    let lib = m.lib.as_ref().unwrap();
    assert!(lib.crate_type.is_empty());

    let serialized = toml::to_string(&m).unwrap();
    assert!(!serialized.contains("crate-type"));

    let m = Manifest::from_slice(serialized.as_bytes()).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert!(lib.crate_type.is_empty());
}

#[test]
fn opt_level() {
    let m = Manifest::from_slice(&read("tests/opt_level.toml").unwrap()).unwrap();
    let package = m.package();
    assert_eq!("byteorder", package.name);
    assert_eq!(3, m.profile.bench.as_ref().unwrap().opt_level.as_ref().unwrap().as_integer().unwrap());
    assert!(!m.lib.as_ref().unwrap().bench);
    assert_eq!(cargo_toml::Edition::E2015, package.edition());
    assert_eq!(1, m.patch.len());
    assert_eq!(Some(StripSetting::Symbols), m.profile.bench.as_ref().unwrap().strip);
}

#[test]
fn autobin() {
    let m = Manifest::from_path("tests/autobin/Cargo.toml").expect("load autobin");
    let package = m.package();
    assert_eq!("auto-bin", package.name);
    assert_eq!(cargo_toml::Edition::E2018, package.edition());
    assert!(package.autobins);
    assert!(m.lib.is_none());

    let mut bins: Vec<(&str, &str)> = m.bin.iter()
        .filter_map(|product| Some((product.name.as_deref()?, product.path.as_deref()?)))
        .collect();
    bins.sort_unstable();

    let mut expected_bins = [
        ("abcde", "src/abcde.rs"),
        ("auto-bin", "src/main.rs"),
        ("a", "src/bin/a.rs"),
        ("b", "src/bin/b.rs"),
        ("c", "src/bin/c/main.rs"),
        ("d", "src/bin/d/main.rs"),
        ("e", "src/bin/e/main.rs"),
    ];
    expected_bins.sort_unstable();

    assert_eq!(bins, expected_bins);

    let bin_e = m.bin.iter()
        .find(|product| product.name.as_deref() == Some("e"))
        .unwrap();

    assert_eq!(&bin_e.required_features, &["feat1"]);
}

#[test]
fn autolib() {
    let m = Manifest::from_path("tests/autolib/Cargo.toml").expect("load autolib");
    let package = m.package();
    assert_eq!("auto-lib", package.name);
    assert_eq!(Path::new("SOMETHING"), package.readme().as_path().unwrap());
    assert_eq!(false, *package.publish.as_ref().unwrap());
    assert_eq!(cargo_toml::Edition::E2015, package.edition());
    assert!(package.autobins);
    assert!(!package.autoexamples);
    let lib = m.lib.unwrap();
    assert_eq!("auto_lib", lib.name.unwrap());
    assert_eq!(lib.crate_type, vec!["lib".to_string()]);
    assert_eq!(0, m.bin.len());
    assert_eq!(Some(StripSetting::None), m.profile.release.unwrap().strip);
    #[allow(deprecated)]
    {
        assert_eq!(m.replace.len(), 2);
    }
}

#[test]
fn autolib2() {
    let m = Manifest::from_path("tests/autolib2/Cargo.toml").expect("load autolib");
    let package = m.package();
    assert_eq!("auto-lib2", package.name);
    assert_eq!(cargo_toml::Edition::E2021, package.edition());
    assert!(m.package().build.is_none());
    assert!(!package.autobins);
    let lib = m.lib.unwrap();
    assert_eq!("auto_lib2", lib.name.unwrap());
    assert_eq!(cargo_toml::Edition::E2018, lib.edition.unwrap());
    assert_eq!(lib.crate_type, vec!["lib".to_string()]);
    assert_eq!(0, m.bin.len());
}

#[test]
fn autolib3() {
    let m = Manifest::from_path("tests/autolib3/Cargo.toml").expect("load autolib");
    let package = m.package();
    assert_eq!("auto-lib3", package.name);
    assert_eq!(cargo_toml::Edition::E2021, package.edition());
    assert!(!package.autobins);

    assert!(matches!(m.package().build.as_ref().unwrap(), cargo_toml::OptionalFile::Flag(false)));
    let lib = m.lib.unwrap();
    assert_eq!("renamed_lib", lib.name.unwrap());
    assert_eq!(cargo_toml::Edition::E2021, lib.edition.unwrap());
    assert_eq!(lib.crate_type, vec!["lib".to_string()]);
    assert_eq!(0, m.bin.len());
}

#[test]
fn autoworkspace() {
    let m = Manifest::from_path("tests/autoworkspace/Cargo.toml").expect("load autoworkspace");
    let workspace = m.workspace.as_ref().unwrap();
    assert_eq!(workspace.members, vec!["autolib"]);
    assert_eq!(workspace.exclude, vec!["nothing"]);
    assert!(workspace.metadata.is_some());
    if let Some(metadata) = &workspace.metadata {
        assert!(metadata.is_table());
        assert_eq!(metadata.get("example_metadata"), Some(&toml::Value::String("expected".into())));
    }
    assert_eq!(workspace.resolver, Some(cargo_toml::Resolver::V2));
}

#[test]
fn proc_macro() {
    let manifest = br#"[package]
    name = "foo"
    version = "1.0.0"
    [lib]
    proc-macro = true
    "#;
    let m = Manifest::from_slice(manifest).unwrap();
    let package = m.package();
    assert_eq!("foo", package.name);
    let lib = m.lib.as_ref().unwrap();
    assert!(lib.crate_type.is_empty());
    assert!(lib.proc_macro);

    let serialized = toml::to_string(&m).unwrap();
    assert!(!serialized.contains("crate-type"));
    assert!(serialized.contains("proc-macro"));

    let m = Manifest::from_slice(serialized.as_bytes()).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert!(lib.crate_type.is_empty());
    assert!(lib.proc_macro);

    // The "proc-macro" field can also be spelled "proc_macro"
    let manifest = br#"[project]
    name = "foo"
    version = "1"
    [lib]
    proc_macro = true
    "#;
    let m = Manifest::from_slice(manifest).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert!(lib.crate_type.is_empty());
    assert!(lib.proc_macro);
}

#[test]
fn serialize() {
    let m = Manifest::from_slice(&read("tests/serialize.toml").unwrap()).unwrap();
    let serialized = toml::to_string(&m);
    assert!(serialized.is_ok());
}

#[test]
fn self_inherit() {
    let mut m = Manifest::from_slice(&read("tests/self-inherit/Cargo.toml").unwrap()).unwrap();
    m.complete_from_path("tests/self-inherit/Cargo.toml".as_ref()).unwrap();
    assert_eq!("1.0.0-lol", m.package().version());

    let m = Manifest::from_path("tests/self-inherit/Cargo.toml").unwrap();
    assert_eq!("1.0.0-lol", m.package().version());
}

#[test]
fn serialize_virtual_manifest() {
    let manifest = br#"[workspace]
    members = [
        "autobin",
        "autolib",
    ]
    "#;
    let m = Manifest::from_slice(manifest).unwrap();
    let serialized = toml::to_string(&m).unwrap();
    assert_eq!(serialized, ["[workspace]", "members = [\"autobin\", \"autolib\"]", "",].join("\n"),);
    assert!(Manifest::from_str(&serialized).is_ok());
}

#[test]
fn inherit() {
    let ws = Manifest::from_slice(&read("tests/inheritance/Cargo.toml").unwrap()).unwrap();
    assert_eq!(3, ws.workspace.as_ref().unwrap().dependencies.len());
    let mut m = Manifest::from_slice(&read("tests/inheritance/hi/Cargo.toml").unwrap()).unwrap();
    assert_eq!(5, m.dependencies.len());
    m.complete_from_path_and_workspace(Path::new("tests/inheritance/hi/Cargo.toml"), Some((&ws, Path::new("root")))).unwrap();

    let otherdep_detail = &m.dependencies.get("otherdep").unwrap().detail().unwrap();
    assert!(otherdep_detail.inherited);
    assert_eq!(["foo", "bar"], otherdep_detail.features[..]);
    assert_eq!(Some("root/workspace-relative"), otherdep_detail.path.as_deref());
    assert_eq!(Path::new("root/ws-path/readme"), m.package().readme().as_path().unwrap());
    assert_eq!(Path::new("root/ws-lic"), m.package().license_file().unwrap());

    let path_dep = &m.dependencies.get("path_dep").unwrap().detail().unwrap();
    assert_eq!(Some("leaf-relative"), path_dep.path.as_deref());
    assert!(!path_dep.inherited);
}

#[test]
fn inherit_doubly_nested() {
    let manifest = Manifest::from_path("tests/inheritance/hi/doubly_nested/Cargo.toml").unwrap();
    assert_eq!("2.2.0", manifest.package().version());
}

#[test]
fn auto_inherit() {
    let m = Manifest::from_path("tests/inheritance/hi/Cargo.toml").unwrap();
    assert_eq!(5, m.dependencies.len());

    assert_eq!(["foo", "bar"], &m.dependencies.get("otherdep").unwrap().detail().unwrap().features[..]);
    assert_eq!(Path::new("tests/inheritance/ws-path/readme"), m.package().readme().as_path().unwrap());
    assert_eq!(Path::new("tests/inheritance/ws-lic"), m.package().license_file().unwrap());

    let m = Manifest::from_path(Path::new("tests/inheritance/hi/Cargo.toml").canonicalize().unwrap()).unwrap();
    assert_eq!(5, m.dependencies.len());

    assert!(m.package().readme().as_path().unwrap().to_string_lossy().contains("/tests/inheritance/ws-path/readme"));
}

#[test]
fn auto_inherit2() {
    let m = Manifest::from_path("tests/inheritance/with-dir/Cargo.toml").unwrap();
    assert_eq!("1.0.0-lol", m.package().version());
}

#[test]
fn renamed_lib() {
    let m = Manifest::from_slice(&read("tests/renamed_lib/Cargo.toml").unwrap()).unwrap();
    let package = m.package();
    assert_eq!("renamed_lib", package.name);
    let lib = m.lib.as_ref().unwrap();
    assert_eq!("librenamed", lib.name.as_ref().unwrap());

    let m = Manifest::from_path("tests/renamed_lib/Cargo.toml").unwrap();
    let package = m.package();
    assert_eq!("renamed_lib", package.name);
    let lib = m.lib.as_ref().unwrap();
    assert_eq!("librenamed", lib.name.as_ref().unwrap());
}

#[test]
fn unstable() {
    let m = Manifest::from_slice(&read("tests/unstable/Cargo.toml").unwrap()).unwrap();
    let dependency = &m.dependencies.get("foo").unwrap().detail().unwrap();
    assert_eq!(dependency.unstable.get("artifact"), Some(&toml::Value::String("bin".into())));

    assert_eq!("0.0.0", m.package().version());
    assert_eq!(false, m.package().publish());

    assert!(m.features["foo"].is_empty());
    assert_eq!(m.features["bar"].as_slice(), &["foo"]);
    assert_eq!(m.features["baz"].as_slice(), &["foo"]);
    assert!(m.features["qux"].is_empty());
    assert_eq!(m.features["quux"], &["dep:foo"]);
}

#[test]
fn edition() {
    let m = Manifest::from_slice(&read("tests/edition/Cargo.toml").unwrap()).unwrap();
    let package = m.package();

    assert_eq!(cargo_toml::Edition::E2024, package.edition());
}

#[test]
fn lints() {
    let m = Manifest::from_slice(&read("tests/lints/Cargo.toml").unwrap()).unwrap();

    let lints = m.workspace.unwrap().lints.unwrap();
    let lint_group = lints.get("rust").unwrap();
    assert_eq!(lint_group.get("unsafe"), Some(&Lint::Simple(LintLevel::Forbid)));
    assert_eq!(lint_group.get("unknown-rule"), Some(&Lint::Detailed{
        level: LintLevel::Allow,
        priority: Some(-1)
    }));


    let lints = m.lints.unwrap();
    assert!(lints.workspace);
    let lint_group = lints.groups.get("rust").unwrap();
    assert_eq!(lint_group.get("unsafe"), Some(&Lint::Simple(LintLevel::Allow)));
    assert_eq!(lint_group.get("unknown-rule"), Some(&Lint::Detailed{
        level: LintLevel::Forbid,
        priority: None
    }));
}
