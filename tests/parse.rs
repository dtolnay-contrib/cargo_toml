use std::path::Path;
use cargo_toml::{Manifest, StripSetting};
use std::fs::read;

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
    bins.sort();

    let mut expected_bins = [
        ("abcde", "src/abcde.rs"),
        ("auto-bin", "src/main.rs"),
        ("a", "src/bin/a.rs"),
        ("b", "src/bin/b.rs"),
        ("c", "src/bin/c/main.rs"),
        ("d", "src/bin/d/main.rs"),
        ("e", "src/bin/e/main.rs"),
    ];
    expected_bins.sort();

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
    assert_eq!(lib.crate_type, vec!["rlib".to_string()]);
    assert_eq!(0, m.bin.len());
    assert_eq!(Some(StripSetting::None), m.profile.release.unwrap().strip);
    #[allow(deprecated)]
    {
        assert_eq!(m.replace.len(), 2);
    }
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
fn legacy() {
    let m = Manifest::from_slice(
        br#"[project]
                name = "foo"
                version = "1"
                "#,
    )
    .expect("parse old");
    let package = m.package();
    assert_eq!("foo", package.name);
    let m = Manifest::from_str("name = \"foo\"\nversion=\"1\"").expect("parse bare");
    let package = m.package();
    assert_eq!("foo", package.name);
}

#[test]
fn proc_macro() {
    let manifest = br#"[project]
    name = "foo"
    version = "1"
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
    assert_eq!(2, ws.workspace.as_ref().unwrap().dependencies.len());
    let mut m = Manifest::from_slice(&read("tests/inheritance/hi/Cargo.toml").unwrap()).unwrap();
    assert_eq!(3, m.dependencies.len());
    m.inherit_workspace(&ws, Path::new("root")).unwrap();

    assert_eq!(["foo", "bar"], &m.dependencies.get("otherdep").unwrap().detail().unwrap().features[..]);
    assert_eq!(Path::new("root/ws-path/readme"), m.package().readme().as_path().unwrap());
    assert_eq!(Path::new("root/ws-lic"), m.package().license_file().unwrap());
}
