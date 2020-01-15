#![cfg(windows)]
#![feature(test)]
extern crate test;

use lazy_static::lazy_static;
use std::io::{Error, ErrorKind, Result};
use std::path::*;

fn aojdk13() -> Result<PathBuf> {
    let aojdk = native_program_files()?.join("AdoptOpenJDK");
    for dir in aojdk.read_dir()? {
        let dir = dir?;
        if dir.file_name().to_string_lossy().starts_with("jdk-13.") { return Ok(dir.path()); }
    }
    Err(Error::new(ErrorKind::NotFound, "Couldn't find an AdoptOpenJDK 13 installation of the same architecture"))
}

fn jdk13() -> Result<PathBuf> {
    let jdk = native_program_files()?.join("Java").join("jdk13");
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

lazy_static! {
    static ref JDK13        : PathBuf = aojdk13().or_else(|_| jdk13()).expect("Expected a JDK 13 or AdoptOpenJDK 13 installation of the same architecture to test against");
    static ref JIMAGE_LIB   : PathBuf = JDK13.join("bin").join(jimage::Library::NAME);
    static ref MODULES      : PathBuf = JDK13.join("lib").join("modules");
}

#[bench] pub fn b0_load_library(b: &mut ::test::Bencher) {
    let lib = &*JIMAGE_LIB;
    b.iter(||{
        let _lib = jimage::Library::load(lib).unwrap();
    });
}

#[bench] pub fn b1_load_mods(b: &mut ::test::Bencher) {
    let lib = jimage::Library::load(&*JIMAGE_LIB).unwrap();
    let mods = &*MODULES;
    b.iter(|| {
        let _mods = lib.open(mods).unwrap();
    });
}

#[bench] pub fn b2_enum_mods_from_scratch(b: &mut ::test::Bencher) {
    let lib = jimage::Library::load(&*JIMAGE_LIB).unwrap();
    b.iter(|| {
        let mods = lib.open(&*MODULES).unwrap();
        let mut classes = 0;
        mods.visit(|r|{
            if r.extension_cstr().to_bytes() == b"class" {
                classes += 1;
            }
            jimage::VisitResult::Continue
        });
        std::hint::black_box(classes);
    });
}

#[bench] pub fn b3_enum_mods_with_reuse(b: &mut ::test::Bencher) {
    let lib = jimage::Library::load(&*JIMAGE_LIB).unwrap();
    let mods = lib.open(&*MODULES).unwrap();
    b.iter(|| {
        let mut classes = 0;
        mods.visit(|r|{
            if r.extension_cstr().to_bytes() == b"class" {
                classes += 1;
            }
            jimage::VisitResult::Continue
        });
        std::hint::black_box(classes);
    });
}
