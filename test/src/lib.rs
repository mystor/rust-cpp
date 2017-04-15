#![cfg_attr(not(test), allow(dead_code, unused_imports))]

#[macro_use]
extern crate cpp;

#[cfg(test)]
mod inner;

cpp!{{
    #define _USE_MATH_DEFINES
    #include <math.h>
}}

#[test]
fn captures() {
    let x: i32 = 10;
    let mut y: i32 = 20;
    let z = unsafe {
        cpp! {[x as "int", mut y as "int"] -> i64 as "long long int" {
            y += 1;
            return x + y;
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
