# rust-cpp

Rust-cpp is an experimental compiler plugin for the rust programming language which enables you to write C++ code inline in your rust code.

## WARNING: Highly Unstable

The API and functionality of this compiler plugin is very experimental and unstable. If you decide to use it in one of your applications, be prepared for it to break irreparably, or for the API to change dramatically, in a single dot release, or due to a rustc update.

## Usage

Import C++ header files

```rust
cpp_include!(<memory>);
cpp_include!(<vector>);
cpp_include!("my_header.h");
```

Write C++ classes & structs

```rust
cpp_header!{
    class Foo {
        Foo() {}
    };
}
```

Run C++ code inline in your rust code

```rust
let foo = 1i32;
unsafe {
    let bar = cpp!((mut foo) -> i32 {
        foo++;
        std::vector<rs::i32> a;
        a.push_back(foo);
        a.push_back(foo + 5);
        return a[0] + a[1];
    });

    assert_eq!(foo, 2);
    assert_eq!(bar, 9);
}
```

rust-cpp automagically generates struct declarations in C++ for your structs in rust!

```rust
#[repr(C)]
struct Foo {
    i: i32,
    j: u64,
}

fn main() {
    unsafe {
        let a = Foo { i: 10, j: 20 };
        let b = cpp!((a) -> i32 {
            return a.i;
        });
        assert_eq!(b, 10);
    }
}
```

You can also declare your own!

```rust
#[cpp_type = "std::vector<uint32_t>"]
enum Vector {}

fn main() {
    unsafe {
        let vecref = cpp!(() -> *mut Vector {
            return new std::vector<uint32_t>;
        });
    }
}
```

## Limitations

1. This only runs on nightly, as it is a compiler plugin.
2. This is very likely to break, as it depends on the current control flow mechanisms in the compiler, which are somewhat likely to change (especially when incremental compilation lands). 

