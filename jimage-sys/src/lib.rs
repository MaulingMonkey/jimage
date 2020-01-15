#![cfg_attr(feature = "nightly", feature(external_doc)  )] // https://doc.rust-lang.org/unstable-book/language-features/external-doc.html
#![cfg_attr(feature = "nightly", doc(include = "../Readme.md"))]
#![allow(non_snake_case)]

// This file is hand-authored Rust authored by MaulingMonkey.  It contains some
// symbol names and error constants originally sourced from jimage.hpp[1].
// Out of an abundance of paranoia and caution, I've included Oracle's copyright
// notice and license terms (3-Clause BSD) - despite what little, if any,
// copyright claim they might have to this file probably being covered under
// fair use anyways - and released my modifications under the same license terms
// for simplicity's sake.
// 
// Oracle does not endorse this project, nor any derived products.
// 
// [1]: https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/master/src/java.base/share/native/libjimage/jimage.hpp

/*
 * Copyright (c) 2019, jimage-sys contributors. All rights reserved.
 * Copyright (c) 2015, 2019, Oracle and/or its affiliates. All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 *   - Redistributions of source code must retain the above copyright
 *     notice, this list of conditions and the following disclaimer.
 *
 *   - Redistributions in binary form must reproduce the above copyright
 *     notice, this list of conditions and the following disclaimer in the
 *     documentation and/or other materials provided with the distribution.
 *
 *   - Neither the name of Oracle nor the names of its
 *     contributors may be used to endorse or promote products derived
 *     from this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
 * IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
 * THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE COPYRIGHT OWNER OR
 * CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
 * EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
 * PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
 * PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
 * LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
 * NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
 * SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

use jni_sys::{jint, jlong};
use std::convert::TryFrom;
use std::ffi::c_void;
use std::io;
use std::os::raw::*;
use std::path::*;

/// Opaque handle to a jimage-format file opened by jimage.dll
pub struct JImageFile { _private: [u8; 0] }

/// The location of a resource within a jimage-format file
pub type JImageLocationRef = jlong;

#[doc = "Error code"] pub const JIMAGE_NOT_FOUND      : jint =  0;
#[doc = "Error code"] pub const JIMAGE_BAD_MAGIC      : jint = -1;
#[doc = "Error code"] pub const JIMAGE_BAD_VERSION    : jint = -2;
#[doc = "Error code"] pub const JIMAGE_CORRUPTED      : jint = -3;

/// Raw callback for JIMAGE_ResourceIterator
pub type JImageResourceVisitor = unsafe extern "C" fn (image: *mut JImageFile, module_name: *const c_char, version: *const c_char, package: *const c_char, name: *const c_char, extension: *const c_char, arg: *mut c_void) -> bool;

/// jimage.dll fns / entry points.  See [jimage.hpp](https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/f3283b6e2d7676423a23c372754ceef7d2ee731f/src/java.base/share/native/libjimage/jimage.hpp) for more details
pub struct Library {
    // Safety:  These symbols must all exactly match those found in jimage.hpp
    // 
    // Definitions of these symbols:    https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/f3283b6e2d7676423a23c372754ceef7d2ee731f/src/java.base/share/native/libjimage/jimage.hpp
    // Definitions of JNIEXPORT:        https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/bb0786d980437800b9d6efe17e42d18241714ea1/src/java.base/windows/native/include/jni_md.h#L29-L30
    //                                  https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/bb0786d980437800b9d6efe17e42d18241714ea1/src/java.base/unix/native/include/jni_md.h#L29-L43
    // 
    // Note that these symbols are *NOT* JNICALL, as such I believe extern "C" is correct here.
    // Testing verifies that extern "C" at least doesn't crash, whereas extern "system" will crash i686 tests.
    pub JIMAGE_Open:                unsafe extern "C" fn (name: *const c_char, error: *mut jint) -> *mut JImageFile,
    pub JIMAGE_Close:               unsafe extern "C" fn (image: *mut JImageFile) -> (),
    pub JIMAGE_PackageToModule:     unsafe extern "C" fn (image: *mut JImageFile, package_name: *const c_char) -> *const c_char,
    pub JIMAGE_FindResource:        unsafe extern "C" fn (image: *mut JImageFile, module_name: *const c_char, version: *const c_char, name: *const c_char, size: *mut jlong) -> JImageLocationRef,
    pub JIMAGE_GetResource:         unsafe extern "C" fn (image: *mut JImageFile, location: JImageLocationRef, buffer: *mut c_char, size: jlong) -> jlong,
    pub JIMAGE_ResourceIterator:    unsafe extern "C" fn (image: *mut JImageFile, visitor: JImageResourceVisitor, arg: *mut c_void) -> (),
    // JIMAGE_ResourcePath was removed: https://github.com/AdoptOpenJDK/openjdk-jdk13u/commit/6b65be6168bcfe398032d33947bcce391f36bba7
}

impl Library {
    /// Load a given libjimage.so or jimage.dll path.
    pub fn load(path: &Path) -> io::Result<Self> {
        Self::from(minidl::Library::load(path)?)
    }

    /// Load symbols from an already loaded DLL or SO file.
    pub fn from(lib: minidl::Library) -> io::Result<Self> {
        // Safety:  These transmute, soundness requires the JIMAGE_* symbols match those of the structure exactly.
        unsafe{Ok(Self{
            JIMAGE_Open:                lib.sym("JIMAGE_Open\0")?,
            JIMAGE_Close:               lib.sym("JIMAGE_Close\0")?,
            JIMAGE_PackageToModule:     lib.sym("JIMAGE_PackageToModule\0")?,
            JIMAGE_FindResource:        lib.sym("JIMAGE_FindResource\0")?,
            JIMAGE_GetResource:         lib.sym("JIMAGE_GetResource\0")?,
            JIMAGE_ResourceIterator:    lib.sym("JIMAGE_ResourceIterator\0")?,
        })}
    }
}

impl TryFrom<minidl::Library> for Library {
    type Error = io::Error;
    fn try_from(lib: minidl::Library) -> io::Result<Self> {
        Library::from(lib)
    }
}
