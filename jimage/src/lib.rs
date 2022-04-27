#![cfg_attr(feature = "nightly", feature(external_doc)  )] // https://doc.rust-lang.org/unstable-book/language-features/external-doc.html
#![cfg_attr(feature = "nightly", doc(include = "../Readme.md"))]

use jimage_sys as sys;
use jni_sys::jlong;
use std::fmt::Display;
use std::ffi::{c_void, CStr};
use std::io::ErrorKind;
use std::ops::Drop;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;

/// A re-export of [std::io::Error](https://doc.rust-lang.org/std/io/struct.Error.html)
pub type Error = std::io::Error;

/// A re-export of [std::io::Result](https://doc.rust-lang.org/std/io/type.Result.html)
pub type Result<T> = std::io::Result<T>;



/// A loaded jimage library such as `jimage.dll` or `libjimage.so`
/// 
/// ## Soundness
/// 
/// The soundness of this type relies on a few assumptions:
/// * Any libraries passed to this, are well-formed.
/// * Any library exposing all of the expected `JIMAGE_*` symbols we expect,
///   does so with the right function signatures.
/// * The underlying C code we call into is sound.
///   Bugs in `jimage.dll`, or a bogus implementation, could violate this assumption.
/// 
/// As a failure to meet any of these assumptions would reasonably be considered
/// a bug with that library - not this crate - I consider it reasonable to have
/// the relevant loading methods marked safe.  [Box::new] is marked safe despite
/// similar hypotheticals involving bogus libc.so s with misdefined malloc symbols.
/// 
/// [Box::new]:         https://doc.rust-lang.org/std/boxed/struct.Box.html#method.new
pub struct Library(Arc<sys::Library>);

impl Library {
    /// The typical, expected name of the library on this platform - e.g. `"jimage.dll"` or `"libjimage.so"`
    pub const NAME : &'static str = Self::_NAME;
    #[cfg(windows)] const _NAME : &'static str = "jimage.dll";
    #[cfg(unix)]    const _NAME : &'static str = "libjimage.so";

    /// Load a jimage library such as `jdk-13.0.1.9-hotspot/bin/jimage.dll`
    pub fn load(path: impl AsRef<Path>) -> Result<Self> { Ok(Self(Arc::new(sys::Library::load(path.as_ref())?))) }

    /// Open a jimage-format file such as `jdk-13.0.1.9-hotspot/lib/modules`
    pub fn open(&self, path: impl AsRef<Path>) -> Result<File> { File::open(self, path) }
}

/// A loaded jimage file such as `jdk-13.0.1.9-hotspot/lib/modules`
pub struct File {
    api:    Arc<sys::Library>,
    file:   AssertThreadSafe<*mut sys::JImageFile>,
}
fn _assert_file_is_send(file: &File) -> &dyn Send { file }
fn _assert_file_is_sync(file: &File) -> &dyn Sync { file }

impl File {
    /// Open a jimage-format file such as `jdk-13.0.1.9-hotspot/lib/modules`
    pub fn open(api: &Library, path: impl AsRef<Path>) -> Result<Self> {
        let orig = path.as_ref();
        let path = orig.to_str().ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("File::open(api, {:?}) failed: couldn't convert path UTF8", orig)))?;
        let mut path = path.bytes().map(|b| b as c_char).collect::<Vec<c_char>>();
        path.push(0);

        let mut err = 0;
        let file = unsafe { (api.0.JIMAGE_Open)(path.as_ptr(), &mut err) };
        if file == null_mut() { return Err(ji2io(format!("File::open(api, {:?}) failed", path), err)); }

        // Safety:  I've taken a quick audit of jimage's C++ source code.  Once you look past the initial C entry
        // points, it quickly starts using `const` appropriately.  Parsing is up front, all the getters are nice and
        // const-qualified, and I see no mutable keywords nor const_cast s.  While something still could've slipped by
        // me, I'm relatively comfortable labeling the file APIs called on an open file thread safe.
        // 
        //  References:
        // https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/f3283b6e2d7676423a23c372754ceef7d2ee731f/src/java.base/share/native/libjimage/jimage.cpp
        // https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/f3283b6e2d7676423a23c372754ceef7d2ee731f/src/java.base/share/native/libjimage/imageFile.hpp
        // https://github.com/AdoptOpenJDK/openjdk-jdk13u/blob/f3283b6e2d7676423a23c372754ceef7d2ee731f/src/java.base/share/native/libjimage/imageFile.cpp
        let file = unsafe { AssertThreadSafe::new(file) };

        Ok(Self{
            api: Arc::clone(&api.0),
            file,
        })
    }

    /// Map a package ("java/lang") to a module ("java.base")
    pub fn package_to_module<'s>(&'s self, package_name: &CStr) -> Result<&'s CStr> {
        let result = unsafe { (self.api.JIMAGE_PackageToModule)(*self.file, package_name.as_ptr()) };
        if result != null() {
            Ok(unsafe { CStr::from_ptr(result) }) // C string lasts as long as th file does
        } else {
            Err(Error::new(ErrorKind::NotFound, format!("file.package_to_module({:?}) failed: no such package", package_name)))
        }
    }

    /// Map a module ("java.base"), version ("9.0"), and name ("java/lang/Object.class") to a size + location.
    pub fn find_resource<'s>(&'s self, module_name: &CStr, version: &CStr, name: &CStr) -> Result<Resource<'s>> {
        let mut size = 0;
        let result = unsafe { (self.api.JIMAGE_FindResource)(*self.file, module_name.as_ptr(), version.as_ptr(), name.as_ptr(), &mut size) };
        if result <= 0 {
            Err(ji2io(format!("file.find_resource({:?}, {:?}, {:?}) failed", module_name, version, name), result))
        } else {
            Ok(Resource{
                file:       self,
                location:   result,
                size:       size as u64,
            })
        }
    }

    /// Enumerate all resources of the file so long as the callback returns VisitResult::Continue.
    pub fn visit<F: FnMut(VisitParams) -> VisitResult>(&self, f: F) {
        unsafe extern "C" fn visit<F: FnMut(VisitParams) -> VisitResult>(_image: *mut sys::JImageFile, module_name: *const c_char, version: *const c_char, package: *const c_char, name: *const c_char, extension: *const c_char, arg: *mut c_void) -> bool {
            let context = &mut *(arg as *mut VisitContext::<F>);
            (context.f)(VisitParams {
                file:           context.file,
                module_name:    CStr::from_ptr(module_name),
                version:        CStr::from_ptr(version),
                package:        CStr::from_ptr(package),
                name:           CStr::from_ptr(name),
                extension:      CStr::from_ptr(extension),
            }) == VisitResult::Continue
        }
        let mut context = VisitContext {
            file: self,
            f,
        };
        let context : *mut VisitContext::<F> = &mut context;
        unsafe { (self.api.JIMAGE_ResourceIterator)(*self.file, visit::<F>, context as *mut c_void) };
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe { (self.api.JIMAGE_Close)(*self.file) };
    }
}

/// The location and size of a jimage resource such as `java/lang/Object.class`
pub struct Resource<'file> {
    file:       &'file File,
    location:   sys::JImageLocationRef,
    size:       u64,
    // I don't know if it's sound to mix sys::JImageLocationRef s with different files.
    // As such, this resource struct bundles it directly with the file that it belongs to,
    // making it impossible to use it with the wrong file, or for it to outlive the file
    // in question.
}

impl Resource<'_> {
    /// How large this resource is in bytes
    pub fn size(&self) -> u64 { self.size }

    /// Read the raw bytes of this resource into the given buffer
    pub fn get(&self, buffer: &mut [u8]) -> Result<u64> {
        let len = (buffer.len() as u64).min(std::i64::MAX as u64) as i64;
        let result = unsafe { (self.file.api.JIMAGE_GetResource)(*self.file.file, self.location, buffer.as_mut_ptr() as *mut _, len) };
        if result < 0 {
            Err(ji2io("resource.get(...) failed", result))
        } else {
            Ok(result as u64)
        }
    }
}

/// The parameters to [File::visit]
/// 
/// [File::visit]:          struct.File.html#method.visit
pub struct VisitParams<'file> {
    file:           &'file File,
    module_name:    &'file CStr,
    version:        &'file CStr,
    package:        &'file CStr,
    name:           &'file CStr,
    extension:      &'file CStr,
}

impl<'file> VisitParams<'file> {
    /// The module name (e.g. `"java.base"`)
    pub fn module_name_cstr(&self)  -> &'file CStr { self.module_name }
    /// The module version (e.g. `"9"` or `"9.0"`)
    pub fn version_cstr(&self)      -> &'file CStr { self.version }
    /// The package (e.g. `"java/lang"`)
    pub fn package_cstr(&self)      -> &'file CStr { self.package }
    /// The name (e.g. `"OuterClass$InnerClass"`)
    pub fn name_cstr(&self)         -> &'file CStr { self.name }
    /// The file extension (e.g. `"class"`)
    pub fn extension_cstr(&self)    -> &'file CStr { self.extension }

    /// The module name (e.g. `"java.base"`)
    pub fn module_name(&self)   -> Result<&'file str> { self.module_name    .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("module_name {:?} isn't valid UTF8",   self.module_name  ))) }
    /// The module version (e.g. `"9"` or `"9.0"`)
    pub fn version(&self)       -> Result<&'file str> { self.version        .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("version {:?} isn't valid UTF8",       self.version      ))) }
    /// The package (e.g. `"java/lang"`)
    pub fn package(&self)       -> Result<&'file str> { self.package        .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("package {:?} isn't valid UTF8",       self.package      ))) }
    /// The name (e.g. `"OuterClass$InnerClass"`)
    pub fn name(&self)          -> Result<&'file str> { self.name           .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("name {:?} isn't valid UTF8",          self.name         ))) }
    /// The file extension (e.g. `"class"`)
    pub fn extension(&self)     -> Result<&'file str> { self.extension      .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("extension {:?} isn't valid UTF8",     self.extension    ))) }

    /// Get a resource handle allowing you to read the file in question
    pub fn resource(&self) -> Result<Resource<'file>> {
        self.file.find_resource(self.module_name, self.version, CStr::from_bytes_with_nul(format!(
            "{}/{}.{}\0",
            self.package    .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("resource package name {:?} isn't valid UTF8",   self.package    )))?,
            self.name       .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("resource name {:?} isn't valid UTF8",           self.name       )))?,
            self.extension  .to_str().map_err(|_| Error::new(ErrorKind::InvalidData, format!("resource extension {:?} isn't valid UTF8",      self.extension  )))?,
        ).as_bytes()).unwrap())
    }
}

/// If [File::visit] should Cancel or Continue visiting more of the [File]
/// 
/// [File]:                 struct.File.html
/// [File::visit]:          struct.File.html#method.visit
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum VisitResult {
    Cancel,
    Continue,
}

struct VisitContext<'file, F: FnMut(VisitParams) -> VisitResult> {
    file:   &'file File,
    f:      F,
}

use ats::AssertThreadSafe;
mod ats {
    /// Assert that an individual field is "thread safe" (Sync + Send).  To make this type sound, this
    /// is given a dedicated mod, forcing all users to use the `unsafe fn new` to construct this type.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] #[repr(transparent)] pub(super) struct AssertThreadSafe<T>(T);
    impl<T> AssertThreadSafe<T> { pub unsafe fn new(value: T) -> Self { Self(value) } }
    unsafe impl<T> Send for AssertThreadSafe<T> {}
    unsafe impl<T> Sync for AssertThreadSafe<T> {}
    impl<T> std::ops::Deref     for AssertThreadSafe<T> { fn deref    (&    self) -> &    Self::Target { &    self.0 } type Target = T; }
    impl<T> std::ops::DerefMut  for AssertThreadSafe<T> { fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 } }
}

fn ji2io(prefix: impl Display, err: impl Into<jlong>) -> Error {
    const JIMAGE_NOT_FOUND      : jlong = sys::JIMAGE_NOT_FOUND as jlong;
    const JIMAGE_BAD_MAGIC      : jlong = sys::JIMAGE_BAD_MAGIC as jlong;
    const JIMAGE_BAD_VERSION    : jlong = sys::JIMAGE_BAD_VERSION as jlong;
    const JIMAGE_CORRUPTED      : jlong = sys::JIMAGE_CORRUPTED as jlong;

    match err.into() {
        JIMAGE_NOT_FOUND    => Error::new(ErrorKind::NotFound,      format!("{}: JIMAGE_NOT_FOUND",     prefix)),
        JIMAGE_BAD_MAGIC    => Error::new(ErrorKind::InvalidData,   format!("{}: JIMAGE_BAD_MAGIC",     prefix)),
        JIMAGE_BAD_VERSION  => Error::new(ErrorKind::InvalidData,   format!("{}: JIMAGE_BAD_VERSION",   prefix)),
        JIMAGE_CORRUPTED    => Error::new(ErrorKind::InvalidData,   format!("{}: JIMAGE_CORRUPTED",     prefix)),
        other               => Error::new(ErrorKind::Other,         format!("{}: JIMAGE_??? ({})",      prefix, other)),
    }
}
