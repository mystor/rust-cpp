#![recursion_limit = "512"]
#![cfg_attr(not(test), allow(dead_code, unused_imports))]

use cpp::{cpp, cpp_class};

#[cfg(test)]
mod inner;

#[cfg(test)]
mod inner_sibling;

// Test that module resolution works correctly with inline modules.
#[cfg(test)]
mod nomod {
    pub mod inner;
}

// This non-existent module should not be parsed
#[cfg(feature = "non_existent")]
mod non_existent;

// This module with invalid cpp code should not be parsed
#[cfg(feature = "non_existent")]
mod invalid_code;

fn add_two(x: i32) -> i32 {
    x + 2
}

mod examples;

cpp! {{
    #define _USE_MATH_DEFINES
    #include <math.h>
    #include "src/header.h"
    #include <map>
    #include <iostream>

    int global_int;

    int callRust1(int x)  {
        return rust!(addTwoCallback [x : i32 as "int"] -> i32 as "int" { add_two(x) });
    }
    void *callRust2(void *ptr)  {
        int a = 3;
        typedef int LocalInt;
        typedef void * VoidStar;
        return rust!(ptrCallback [ptr : *mut u32 as "void*", a : u32 as "LocalInt"]
                -> *mut u32 as "VoidStar" {
            unsafe {*ptr += a};
            ptr
        });
    }
    int callRustExplicitReturn(int x) {
        return rust!(explicitReturnCallback [x : i32 as "int"] -> i32 as "int" {
            if x == 0 {
                return 42;
            }
            x + 1
        });
    }
}}

cpp_class!(
    /// Documentation comments
    /** More /*comments*/ */
    pub unsafe struct A as "A");

impl A {
    fn new(a: i32, b: i32) -> Self {
        unsafe {
            return cpp!([a as "int", b as "int"] -> A as "A" {
                return A(a, b);
            });
        }
    }

    fn set_values(&mut self, a: i32, b: i32) {
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

cpp! {{
    bool callRust3(const A &a, int val)  {
        A a2 = rust!(ACallback [a : A as "A", val : i32 as "int"] -> A as "A"
        {
            let mut a2 = a.clone();
            a2.set_values(a.multiply(), val);
            a2
        });
        return a2.a == a.a*a.b && a2.b == val;
    }

    int manyOtherTest() {
        int val = 32;
        int *v = &val;
        // returns void
        rust!(xx___1 [v : &mut i32 as "int*"] { *v = 43; } );
        if (val != 43) return 1;
        rust!(xx___2 [val : &mut i32 as "int&"] { assert!(*val == 43); *val = 54; } );
        if (val != 54) return 2;
        rust!(xx___3 [v : *mut i32 as "int*"] { unsafe {*v = 73;} } );
        if (val != 73) return 3;
        rust!(xx___4 [val : *mut i32 as "int&"] { unsafe { assert!(*val == 73); *val = 62; }} );
        if (val != 62) return 4;
        rust!(xx___5 [val : *const i32 as "const int&"] { unsafe { assert!(*val == 62); }} );
        rust!(xx___6 [val : &i32 as "const int&"] { assert!(*val == 62); } );
        rust!(xx___7 [val : i32 as "int"] { let v = val; assert!(v == 62); } );
        // operations on doubles
        double fval = 5.5;
        double res = rust!(xx___8 [fval : f64 as "double"] -> f64 as "double" { fval * 1.2 + 9.9 } );
        if (int((res - (5.5 * 1.2 + 9.9)) * 100000) != 0) return 5;
        res = rust!(xx___9 [fval : &mut f64 as "double&"] -> f64 as "double" { *fval *= 2.2; 8.8 } );
        if (int((res - (8.8)) * 100000) != 0) return 9;
        if (int((fval - (5.5 * 2.2)) * 100000) != 0) return 10;
        // with a class
        A a(3,4);
        rust!(xx___10 [a : A as "A"] { let a2 = a.clone(); assert!(a2.multiply() == 12); } );
        rust!(xx___11 [a : A as "A"] { let _a = a.clone(); } );
        return 0;
    }
}}

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
    cpp! {unsafe [] { global_int = 33; }};

    let x = unsafe {
        cpp![[] -> i32 as "int" {
            return 10 + global_int;
        }]
    };
    assert_eq!(x, 43);
}

#[test]
fn duplicates() {
    // Test that we can call two captures with the same tokens

    let fn1 = |x| {
        cpp! { unsafe [x as "int"] -> i32 as "int" {
            static int sta;
            sta += x;
            return sta;
        }}
    };
    let fn2 = |x| {
        cpp! { unsafe [x as "int"] -> i32 as "int" {
            static int sta;
            sta += x;
            return sta;
        }}
    };
    assert_eq!(fn1(8), 8);
    assert_eq!(fn1(2), 10);

    // Since both the cpp! inside fn1 and fn2 are made of the same token, the same
    // function is actually generated, meaning they share the same static variable.
    // This might be confusing, I hope nobody relies on this behavior.
    assert_eq!(fn2(1), 11);
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
fn inner_sibling() {
    let x = inner_sibling::inner_sibling();
    assert_eq!(x, 10);
    let y = inner_sibling::child::inner_sibling_child();
    assert_eq!(y, 20);
    let z = inner_sibling::child2::inner_sibling_child2();
    assert_eq!(z, -44);
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
fn test_nomod() {
    assert_eq!(nomod::inner::nomod_inner(), 10);
}

#[test]
fn rust_submacro() {
    let result = unsafe { cpp!([] -> i32 as "int" { return callRust1(45); }) };
    assert_eq!(result, 47); // callRust1 adds 2

    let mut val: u32 = 18;
    {
        let val_ref = &mut val;
        let result = unsafe {
            cpp!([val_ref as "void*"] -> bool as "bool" {
                return callRust2(val_ref) == val_ref;
            })
        };
        assert_eq!(result, true);
    }
    assert_eq!(val, 21); // callRust2 does +=3

    let result = unsafe { cpp!([] -> i32 as "int" { return callRustExplicitReturn(0); }) };
    assert_eq!(result, 42);
    let result = unsafe { cpp!([] -> i32 as "int" { return callRustExplicitReturn(9); }) };
    assert_eq!(result, 10);

    let result = unsafe {
        cpp!([]->bool as "bool" {
            A a(5, 3);
            return callRust3(a, 18);
        })
    };
    assert!(result);

    let result = unsafe {
        cpp!([]->u32 as "int" {
            return manyOtherTest();
        })
    };
    assert_eq!(result, 0);
}

pub trait MyTrait {
    fn compute_value(&self, x: i32) -> i32;
}

cpp! {{
    struct MyClass {
        virtual int computeValue(int) const = 0;
    };
    int operate123(MyClass *callback) { return callback->computeValue(123); }

    struct TraitPtr { void *a,*b; };
}}
cpp! {{
    class MyClassImpl : public MyClass {
      public:
        TraitPtr m_trait;
        int computeValue(int x) const /*override*/ {
           return rust!(MCI_computeValue [m_trait : &dyn MyTrait as "TraitPtr", x : i32 as "int"]
               -> i32 as "int" {
               m_trait.compute_value(x)
           });
       }
   };
}}

struct MyTraitImpl {
    x: i32,
}
impl MyTrait for MyTraitImpl {
    fn compute_value(&self, x: i32) -> i32 {
        self.x + x
    }
}

#[test]
fn rust_submacro_trait() {
    let inst = MyTraitImpl { x: 333 };
    let inst_ptr: &dyn MyTrait = &inst;
    let i = unsafe {
        cpp!([inst_ptr as "TraitPtr"] -> u32 as "int" {
            MyClassImpl mci;
            mci.m_trait = inst_ptr;
            return operate123(&mci);
        })
    };
    assert_eq!(i, 123 + 333);
}

#[test]
fn witin_macro() {
    assert_eq!(unsafe { cpp!([] -> u32 as "int" { return 12; }) }, 12);
    let s = format!("hello{}", unsafe {
        cpp!([] -> u32 as "int" { return 14; })
    });
    assert_eq!(s, "hello14");
}

#[test]
fn with_unsafe() {
    let x = 45;
    assert_eq!(
        cpp!(unsafe [x as "int"] -> u32 as "int" { return x + 1; }),
        46
    );
}

#[test]
fn rust_submacro_closure() {
    let mut result = unsafe {
        cpp!([] -> i32 as "int" {
            auto x = rust!(bbb []-> A as "A" { A::new(5,7) }).multiply();
            auto y = []{ A a(3,2); return rust!(aaa [a : A as "A"] -> i32 as "int" { a.multiply() }); }();
            return x + y;
        })
    };
    assert_eq!(result, 5 * 7 + 3 * 2);

    unsafe {
        cpp!([mut result as "int"] {
            A a(9,2);
            rust!(Ccc [a : A as "A", result : &mut i32 as "int&"] { *result = a.multiply(); });
        })
    };
    assert_eq!(result, 18);
}

pub mod cpp_class;
