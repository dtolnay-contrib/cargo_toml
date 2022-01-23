use cargo_toml::Manifest;
use std::fs::read;

#[test]
fn own() {
    let m = Manifest::from_slice(&read("Cargo.toml").unwrap()).unwrap();
    let package = m.package.as_ref().unwrap();
    assert_eq!("cargo_toml", package.name);
    let m = Manifest::<toml::Value>::from_slice_with_metadata(&read("Cargo.toml").unwrap()).unwrap();
    let package = m.package.as_ref().unwrap();
    assert_eq!("cargo_toml", package.name);
    assert_eq!(cargo_toml::Edition::E2018, package.edition);
    let lib = m.lib.as_ref().unwrap();
    assert_eq!(None, lib.crate_type);

    let serialized = toml::to_string(&m).unwrap();
    assert!(!serialized.contains("crate-type"));

    let m = Manifest::from_slice(serialized.as_bytes()).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert_eq!(None, lib.crate_type);
}

#[test]
fn opt_level() {
    let m = Manifest::from_slice(&read("tests/opt_level.toml").unwrap()).unwrap();
    let package = m.package.as_ref().unwrap();
    assert_eq!("byteorder", package.name);
    assert_eq!(
        3,
        m.profile
            .bench
            .unwrap()
            .opt_level
            .unwrap()
            .as_integer()
            .unwrap()
    );
    assert_eq!(false, m.lib.unwrap().bench);
    assert_eq!(cargo_toml::Edition::E2015, package.edition);
    assert_eq!(1, m.patch.len());
}

#[test]
fn autobin() {
    let m = Manifest::from_path("tests/autobin/Cargo.toml").expect("load autobin");
    let package = m.package.as_ref().unwrap();
    assert_eq!("auto-bin", package.name);
    assert_eq!(cargo_toml::Edition::E2018, package.edition);
    assert!(package.autobins);
    assert!(m.lib.is_none());
    assert_eq!(1, m.bin.len());
    assert_eq!(Some("auto-bin"), m.bin[0].name.as_deref());
}

#[test]
fn autolib() {
    let m = Manifest::from_path("tests/autolib/Cargo.toml").expect("load autolib");
    let package = m.package.as_ref().unwrap();
    assert_eq!("auto-lib", package.name);
    assert_eq!("SOMETHING", package.readme.as_ref().unwrap());
    assert_eq!(false, package.publish);
    assert_eq!(cargo_toml::Edition::E2015, package.edition);
    assert!(package.autobins);
    assert!(!package.autoexamples);
    let lib = m.lib.unwrap();
    assert_eq!("auto_lib", lib.name.unwrap());
    assert_eq!(Some(vec!["rlib".into()]), lib.crate_type);
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
fn legacy() {
    let m = Manifest::from_slice(
        br#"[project]
                name = "foo"
                version = "1"
                "#,
    )
    .expect("parse old");
    let package = m.package.as_ref().unwrap();
    assert_eq!("foo", package.name);
    let m = Manifest::from_str("name = \"foo\"\nversion=\"1\"").expect("parse bare");
    let package = m.package.as_ref().unwrap();
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
    let package = m.package.as_ref().unwrap();
    assert_eq!("foo", package.name);
    let lib = m.lib.as_ref().unwrap();
    assert_eq!(None, lib.crate_type);
    assert_eq!(true, lib.proc_macro);

    let serialized = toml::to_string(&m).unwrap();
    assert!(!serialized.contains("crate-type"));
    assert!(serialized.contains("proc-macro"));

    let m = Manifest::from_slice(serialized.as_bytes()).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert_eq!(None, lib.crate_type);
    assert_eq!(true, lib.proc_macro);

    // The "proc-macro" field can also be spelled "proc_macro"
    let manifest = br#"[project]
    name = "foo"
    version = "1"
    [lib]
    proc_macro = true
    "#;
    let m = Manifest::from_slice(manifest).unwrap();
    let lib = m.lib.as_ref().unwrap();
    assert_eq!(None, lib.crate_type);
    assert_eq!(true, lib.proc_macro);

}

#[test]
fn serialize() {
    let m = Manifest::from_slice(&read("tests/serialize.toml").unwrap()).unwrap();
    let serialized = toml::to_string(&m);
    assert!(serialized.is_ok());
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
    assert_eq!(
        serialized,
        [
            "[workspace]",
            "members = [\"autobin\", \"autolib\"]",
            "",
        ]
        .join("\n"),
    );
    assert!(Manifest::from_str(&serialized).is_ok());
}
