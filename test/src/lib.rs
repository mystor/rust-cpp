#![recursion_limit="512"]
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

fn add_two(x: i32) -> i32 {
    x + 2
}

cpp!{{
    #define _USE_MATH_DEFINES
    #include <math.h>
    #include "src/header.h"
    #include <map>
    #include <iostream>

    int callRust1(int x)  {
        return rust!(addTwoCallback [x : i32 as "int"] -> i32 as "int" { add_two(x) });
    }
    void *callRust2(void *ptr)  {
        int a = 3;
        return rust!(ptrCallback [ptr : *mut u32 as "void*", a : u32 as "int"] -> *mut u32 as "void *"
        { unsafe {*ptr += a}; return ptr; });
    }
}}

cpp_class!(pub unsafe struct A as "A");
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

cpp!{{
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
        res = rust!(xx___9 [fval : &mut f64 as "double&"] -> f64 as "double" { *fval = *fval * 2.2; 8.8 } );
        if (int((res - (8.8)) * 100000) != 0) return 9;
        if (int((fval - (5.5 * 2.2)) * 100000) != 0) return 10;
        // with a class
        A a(3,4);
        rust!(xx___10 [a : A as "A"] { let a2 = a.clone(); assert!(a2.multiply() == 12); } );
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

#[test]
fn rust_submacro() {
    let result = unsafe { cpp!([] -> i32 as "int" { return callRust1(45); }) };
    assert_eq!(result, 47); // callRust1 adds 2

    let mut val : u32 = 18;
    {
        let val_ref = &mut val;
        let result = unsafe { cpp!([val_ref as "void*"] -> bool as "bool" {
            return callRust2(val_ref) == val_ref;
        }) };
        assert_eq!(result, true);
    }
    assert_eq!(val, 21); // callRust2 does +=3

    let result = unsafe { cpp!([]->bool as "bool" {
        A a(5, 3);
        return callRust3(a, 18);
    })};
    assert!(result);

    let result = unsafe { cpp!([]->u32 as "int" {
        return manyOtherTest();
    })};
    assert_eq!(result, 0);
}


pub trait MyTrait {
    fn compute_value(&self, x : i32) -> i32;
}

cpp!{{
    struct MyClass {
        virtual int computeValue(int) const = 0;
    };
    int operate123(MyClass *callback) { return callback->computeValue(123); }

    struct TraitPtr { void *a,*b; };
}}
cpp!{{
    class MyClassImpl : public MyClass {
      public:
        TraitPtr m_trait;
        int computeValue(int x) const /*override*/ {
           return rust!(MCI_computeValue [m_trait : &MyTrait as "TraitPtr", x : i32 as "int"]
               -> i32 as "int" {
               m_trait.compute_value(x)
           });
       }
   };
}}

struct MyTraitImpl {
    x : i32
}
impl MyTrait for MyTraitImpl {
    fn compute_value(&self, x: i32) -> i32 { self.x + x }
}

#[test]
fn rust_submacro_trait() {
    let inst = MyTraitImpl{ x: 333 };
    let inst_ptr : &MyTrait = &inst;
    let i = unsafe { cpp!([inst_ptr as "TraitPtr"] -> u32 as "int" {
        MyClassImpl mci;
        mci.m_trait = inst_ptr;
        return operate123(&mci);
    })};
    assert_eq!(i, 123 + 333);
}

#[test]
fn witin_macro() {
    assert_eq!(unsafe { cpp!([] -> u32 as "int" { return 12; }) }, 12);
    let s = format!("hello{}", unsafe { cpp!([] -> u32 as "int" { return 14; }) } );
    assert_eq!(s, "hello14");
}

#[test]
fn with_unsafe() {
    let x = 45;
    assert_eq!(cpp!(unsafe [x as "int"] -> u32 as "int" { return x + 1; }), 46);
}
