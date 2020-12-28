use cpp::cpp;

pub fn inner_sibling_child2() -> i32 {
    cpp! {unsafe [] -> i32 as "int" {
        return -44;
    }}
}
