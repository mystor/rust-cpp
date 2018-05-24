#![cfg_attr(not(test), allow(dead_code, unused_imports))]

#[macro_use]
extern crate cpp;

#[cfg(test)]
mod inner;

// Test that module resolution works correctly with inline modules.
#[cfg(test)]
mod nomod {
    pub mod inner;
}

cpp!{{
    #define _USE_MATH_DEFINES
    #include <math.h>
    #include "src/header.h"
}}

#[repr(C)]
struct A {
    _opaque: [i32; 2],
}

#[test]
fn captures() {
    let x: i32 = 10;
    let mut y: i32 = 20;
    let z = unsafe {
        cpp! {[x as "int", mut y as "int"] -> i64 as "long long int" {
            y += 1;
            return [&] { return x + y; }();
        }}
    };
    assert_eq!(x, 10);
    assert_eq!(y, 21);
    assert_eq!(z, 31);
}

#[test]
fn no_captures() {
    let x = unsafe {
        cpp![[] -> i32 as "int" {
            return 10;
        }]
    };
    assert_eq!(x, 10);
}

#[test]
fn test_inner() {
    let x = inner::inner();
    assert_eq!(x, 10);
    let y = inner::innerinner::innerinner();
    assert_eq!(y, 10);
    let y = inner::innerpath::explicit_path(10);
    assert_eq!(y, 10);
}

#[test]
fn includes() {
    unsafe {
        let pi = cpp!([] -> f32 as "float" {
            return M_PI;
        });
        assert!(pi - ::std::f32::consts::PI < 0.0000000001);
    }
}

#[test]
fn plusplus() {
    unsafe {
        let mut x: i32 = 0;
        cpp!([mut x as "int"] {
            x++;
        });
        assert_eq!(x, 1);
    }
}

#[test]
fn destructor() {
    unsafe {
        let a = cpp!([] -> A as "A" {
            return A(5, 10);
        });

        let first = cpp!([a as "A"] -> i32 as "int32_t" {
            return a.a;
        });

        assert_eq!(first, 5);
    }
}

#[test]
fn test_nomod() {
    assert_eq!(nomod::inner::nomod_inner(), 10);
}
