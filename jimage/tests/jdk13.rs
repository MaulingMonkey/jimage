#![cfg(windows)]

use std::io::{Error, ErrorKind, Result};
use std::ffi::*;
use std::path::*;

#[test] fn aojdk13_contains_java_lang_object() {
    let aojdk13 = aojdk13().or_else(|_| jdk13()).expect("Expected a JDK 13 or AdoptOpenJDK 13 installation of the same architecture to test against");
    let lib = jimage::Library::load(aojdk13.join("bin").join(jimage::Library::NAME)).unwrap();
    let mods = lib.open(aojdk13.join("lib").join("modules")).unwrap();
    let mut found_object = false;
    mods.visit(|res|{
        if      res.package_cstr().to_bytes()   != b"java/lang" {}
        else if res.name_cstr().to_bytes()      != b"Object"    {}
        else if res.extension_cstr().to_bytes() != b"class"     {}
        else {
            let res = res.resource().expect("Failed to read java/lang/Object.class");
            assert_is_class(&res);
            found_object = true;
        }
        jimage::VisitResult::Continue
    });
    assert!(found_object, "Failed to find java/lang/Object.class");

    let res = mods.find_resource(
        CStr::from_bytes_with_nul(b"java.base\0").unwrap(),
        CStr::from_bytes_with_nul(b"9.0\0").unwrap(),
        CStr::from_bytes_with_nul(b"java/lang/Object.class\0").unwrap(),
    ).expect("Failed to find java/lang/Object.class");
    assert_is_class(&res);
}

fn aojdk13() -> Result<PathBuf> {
    let aojdk = native_program_files()?.join("AdoptOpenJDK");
    for dir in aojdk.read_dir()? {
        let dir = dir?;
        if dir.file_name().to_string_lossy().starts_with("jdk-13.") { return Ok(dir.path()); }
    }
    Err(Error::new(ErrorKind::NotFound, "Couldn't find an AdoptOpenJDK 13 installation of the same architecture"))
}

fn jdk13() -> Result<PathBuf> {
    let jdk = native_program_files()?.join("jdk13");
    if !jdk.exists() {
        Err(Error::new(ErrorKind::NotFound, "Couldn't find a JDK 13 installation of the same architecture"))
    } else {
        Ok(jdk)
    }
}

fn native_program_files() -> Result<PathBuf> {
    let pf = if cfg!(target_arch = "x86_64") { "ProgramW6432" } else { "ProgramFiles(x86)" };
    let pf = std::env::var_os(pf).or(std::env::var_os("ProgramFiles")).ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Neither %{}% nor %ProgramFiles% was set, cannot find AdoptOpenJDK 13", pf)))?;
    let pf = PathBuf::from(pf);
    Ok(pf)
}

fn assert_is_class(res: &jimage::Resource) {
    let mut v = Vec::new();
    v.resize_with(res.size() as usize, || 0);
    assert_eq!(res.get(&mut v[..]).unwrap(), res.size());
    assert!(v.len() >= 4);
    assert_eq!(&v[..4], [0xCA, 0xFE, 0xBA, 0xBE], "java/lang/Object.class didn't have expected prelude magic constant: 0xCAFEBABE");
}
