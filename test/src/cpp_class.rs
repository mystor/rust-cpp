use super::A;

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
    let mut a = A::new(2, 3);
    assert_eq!(a.multiply(), 2 * 3);

    a.set_values(5, 6);
    assert_eq!(a.multiply(), 5 * 6);
}

cpp_class!(pub(crate) unsafe struct B as "B");
impl B {
    fn new(a: i32, b: i32) -> Self {
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
    let mut b = B::new(12, 34);
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
    assert_eq!(mo1.data().multiply(), 8 * 9);
    let mut mo2 = mo1;
    let mo3 = unsafe {
        cpp!([mut mo2 as "MoveOnly"] -> MoveOnly as "MoveOnly" {
            mo2.data.a = 7;
            return MoveOnly(3,2);
        })
    };
    assert_eq!(mo2.data().multiply(), 7 * 9);
    assert_eq!(mo3.data().multiply(), 3 * 2);
}

#[test]
fn derive_eq() {
    cpp! {{
        struct WithOpEq {
            static int val;
            int value = val++;
            friend bool operator==(const WithOpEq &a, const WithOpEq &b) { return a.value == b.value; }
        };
        int WithOpEq::val = 0;
    }};
    cpp_class!(#[derive(Eq, PartialEq)] unsafe struct WithOpEq as "WithOpEq");

    let x1 = WithOpEq::default();
    let x2 = WithOpEq::default();

    assert!(!(x1 == x2));
    assert!(x1 != x2);

    let x3 = x1.clone();
    assert!(x1 == x3);
    assert!(!(x1 != x3));
}

#[test]
fn derive_ord() {
    cpp! {{
        struct Comp {
            int value;
            Comp(int i) : value(i) { }
            friend bool operator<(const Comp &a, const Comp &b) { return a.value < b.value; }
            friend bool operator==(const Comp &a, const Comp &b) { return a.value == b.value; }
        };
    }};
    cpp_class!(#[derive(PartialEq, PartialOrd)] #[derive(Eq, Ord)] unsafe struct Comp as "Comp");
    impl Comp {
        fn new(i: u32) -> Comp {
            unsafe { cpp!([i as "int"] -> Comp as "Comp" { return i; }) }
        }
    }

    let x1 = Comp::new(1);
    let x2 = Comp::new(2);
    let x3 = Comp::new(3);
    assert!(x1 < x2);
    assert!(x2 > x1);
    assert!(x3 > x1);
    assert!(x3 >= x1);
    assert!(x3 >= x3);
    assert!(x2 <= x3);
    assert!(x2 <= x2);
    assert!(!(x1 > x2));
    assert!(!(x2 < x1));
    assert!(!(x3 <= x1));
    assert!(!(x1 < x1));
    assert!(!(x3 > x3));
    assert!(!(x3 < x3));
    assert!(!(x2 >= x3));
}
