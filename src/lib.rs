#[cfg(feature = "build")]
#[macro_use]
extern crate syntex_syntax;

#[cfg(feature = "build")]
extern crate cpp_common;

#[cfg(feature = "build")]
extern crate gcc;

#[cfg(feature = "macro")]
mod mac;

#[cfg(feature = "build")]
mod build;

#[cfg(feature = "build")]
pub use build::*;
