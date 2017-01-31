pub fn explicit_path(im_explicit_path: i32) -> i32 {
    unsafe {
        cpp!([im_explicit_path as "int"] -> i32 as "int" {
            return im_explicit_path;
        })
    }
}
