cpp! {
    fn inner_impl() -> i32 as "int32_t" {
        return 10;
    }
}

pub fn inner() -> i32 {
    unsafe {
        inner_impl()
    }
}
