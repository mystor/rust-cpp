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
    #include <map>
    #include <iostream>
}}

cpp_class!(unsafe struct A as "A");
impl A {
    fn new(a : i32, b: i32) -> Self {
        unsafe {
            return cpp!([a as "int", b as "int"] -> A as "A" {
                return A(a, b);
            });
        }
    }

    fn set_values(&mut self, a : i32, b: i32) {
        unsafe {
            return cpp!([self as "A*", a as "int", b as "int"] {
                self->setValues(a, b);
            });
        }
    }

    fn multiply(&self) -> i32 {
        unsafe {
            return cpp!([self as "const A*"] -> i32 as "int" {
                return self->multiply();
            });
        }
    }
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

        let a1 = a.clone();

        let first = cpp!([a as "A"] -> i32 as "int32_t" {
            return a.a;
        });

        assert_eq!(first, 5);

        let second = cpp!([a1 as "A"] -> i32 as "int32_t" {
            return a1.b;
        });

        assert_eq!(second, 10);
    }
}

#[test]
fn member_function() {
    let mut a = A::new(2,3);
    assert_eq!(a.multiply(), 2*3);

    a.set_values(5,6);
    assert_eq!(a.multiply(), 5*6);
}

cpp_class!(unsafe struct B as "B");
impl B {
    fn new(a : i32, b: i32) -> Self {
        unsafe {
            return cpp!([a as "int", b as "int"] -> B as "B" {
                B ret = { a, b };
                return ret;
            });
        }
    }
    fn a(&mut self) -> &mut i32 {
        unsafe {
            return cpp!([self as "B*"] -> &mut i32 as "int*" {
                return &self->a;
            });
        }
    }
    fn b(&mut self) -> &mut i32 {
        unsafe {
            return cpp!([self as "B*"] -> &mut i32 as "int*" {
                return &self->b;
            });
        }
    }
}


#[test]
fn simple_class() {
    let mut b = B::new(12,34);
    assert_eq!(*b.a(), 12);
    assert_eq!(*b.b(), 34);
    *b.a() = 45;
    let mut b2 = b;
    assert_eq!(*b2.a(), 45);

    let mut b3 = B::default();
    assert_eq!(*b3.a(), 0);
    assert_eq!(*b3.b(), 0);
}

#[test]
fn move_only() {
    cpp_class!(unsafe struct MoveOnly as "MoveOnly");
    impl MoveOnly {
        fn data(&self) -> &A {
            unsafe {
                return cpp!([self as "MoveOnly*"] -> &A as "A*" {
                    return &self->data;
                });
            }
        }
    }
    let mo1 = MoveOnly::default();
    assert_eq!(mo1.data().multiply(), 8*9);
    let mut mo2 = mo1;
    let mo3 = unsafe { cpp!([mut mo2 as "MoveOnly"] -> MoveOnly as "MoveOnly" {
        mo2.data.a = 7;
        return MoveOnly(3,2);
    })};
    assert_eq!(mo2.data().multiply(), 7*9);
    assert_eq!(mo3.data().multiply(), 3*2);
}

#[test]
fn test_nomod() {
    assert_eq!(nomod::inner::nomod_inner(), 10);
}
