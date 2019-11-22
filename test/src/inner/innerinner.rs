use cpp::cpp;

pub fn innerinner() -> i32 {
    unsafe {
        let im_inner_inner: i32 = 10;
        cpp! {[im_inner_inner as "int"] -> i32 as "int" {
            return im_inner_inner;
        }}
    }
}
