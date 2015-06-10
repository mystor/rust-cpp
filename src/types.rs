use std::collections::HashMap;
use std::cell::Cell;

use syntax::ast::{self, Expr};
use syntax::ast::IntTy::*;
use syntax::ast::UintTy::*;
use syntax::ast::FloatTy::*;
use syntax::attr::*;

use rustc::util::ppaux::Repr;
use rustc::middle::ty::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TyState {
    Nothing,
    Declared,
    Defined,
}

struct CppTy {
    decl: Option<String>,
    defn: Option<String>,

    uses_decl: Vec<String>,
    uses_defn: Vec<String>,

    state: Cell<TyState>,
}

pub struct TypeData {
    types: HashMap<String, CppTy>,
    dummy_count: usize,
}

impl TypeData {
    pub fn new() -> TypeData {
        TypeData {
            types: HashMap::new(),
            dummy_count: 0,
        }
    }

    fn gen_decl(&self, buf: &mut String, name: &str) {
        let ty = if let Some(t) = self.types.get(name) { t } else { return };

        let mut cpp = match (ty.state.get(), &ty.decl, &ty.defn) {
            (TyState::Defined, _, _) => return,
            (TyState::Nothing, &Some(ref decl), _) => {
                ty.state.set(TyState::Declared);
                format!("{}\n", decl)
            }
            (_, &None, &Some(ref defn)) => {
                ty.state.set(TyState::Defined);
                for ty in &ty.uses_decl { self.gen_decl(buf, ty); }
                for ty in &ty.uses_defn { self.gen_defn(buf, ty); }
                format!("{}\n", defn)
            }
            _ => return,
        };

        // Wrap the decl in namespaces, such that the namespace is correct
        let mut segs = name.rsplit("::");
        segs.next();
        for seg in segs {
            cpp = format!("namespace {} {{\n{}}}\n", seg, cpp);
        }

        buf.push_str(&cpp);
    }

    fn gen_defn(&self, buf: &mut String, name: &str) {
        let ty = if let Some(t) = self.types.get(name) { t } else { return };

        let mut cpp = match (ty.state.get(), &ty.decl, &ty.defn) {
            (TyState::Defined, _, _) => return,
            (TyState::Nothing, &Some(ref decl), &None) => {
                ty.state.set(TyState::Defined);
                format!("{}\n", decl)
            }
            (_, _, &Some(ref defn)) => {
                ty.state.set(TyState::Defined);
                for ty in &ty.uses_decl { self.gen_decl(buf, ty); }
                for ty in &ty.uses_defn { self.gen_defn(buf, ty); }
                format!("{}\n", defn)
            }
            _ => return,
        };

        // Wrap the defn in namespaces, such that the namespace is correct
        let mut segs = name.rsplit("::");
        segs.next();
        for seg in segs {
            cpp = format!("namespace {} {{\n{}}}\n", seg, cpp);
        }

        buf.push_str(&cpp);
    }

    pub fn to_cpp(&self) -> String {
        let mut s = String::new();
        for name in self.types.keys() { self.gen_defn(&mut s, name); }
        s
    }

    fn add_type(&mut self, name: String, decl: Option<String>, defn: Option<String>,
                uses_decl: Vec<String>, uses_defn: Vec<String>) -> String {
        self.types.insert(name.clone(), CppTy {
            decl: decl,
            defn: defn,
            uses_decl: uses_decl,
            uses_defn: uses_defn,
            state: Cell::new(TyState::Nothing),
        });
        name
    }

    fn void_or_dummy<'tcx>(&mut self, rest: TypeRestrictions) -> Result<String, ()> {
        if rest.can_be_void() {
            Ok(format!("void"))
        } else if rest.incomplete_type_ok() {
            let name = format!("__DummyType_{}", self.dummy_count);
            self.dummy_count += 1;

            Ok(self.add_type(format!("rs::{}", &name),
                             Some(format!("class {};", &name)),
                             None, Vec::new(), Vec::new()))
        } else {
            Err(())
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
enum TypeRestrictions {
    Pointer,
    Reference,
    ByValue,
    Slice,
}

impl TypeRestrictions {
    fn can_be_void(&self) -> bool {
        use self::TypeRestrictions::*;
        match *self {
            Pointer | Slice => true,
            Reference | ByValue => false,
        }
    }

    fn incomplete_type_ok(&self) -> bool {
        use self::TypeRestrictions::*;
        match *self {
            Pointer | Slice | Reference => true,
            ByValue => false,
        }
    }
}


/// Determine the c++ type for a given expression
/// This is the entry point for the types module, and is the intended mechanism for
/// invoking the type translation infrastructure.
pub fn cpp_type_of<'tcx>(td: &mut TypeData,
                         tcx: &ctxt<'tcx>,
                         expr: &Expr,
                         is_arg: bool) -> Result<String, String> {
    // Get the type object
    let rs_ty = expr_ty(tcx, expr);

    if !is_arg {
        // Special case for void return value
        if let ty_tup(ref it) = rs_ty.sty {
            if it.len() == 0 {
                return Ok(format!("void"));
            }
        }
    }

    let restrictions = if is_arg {
        TypeRestrictions::Reference
    } else {
        TypeRestrictions::ByValue
    };

    cpp_type_of_internal(td, tcx, expr, rs_ty, restrictions)
}

fn cpp_type_of_internal<'tcx>(td: &mut TypeData,
                              tcx: &ctxt<'tcx>,
                              expr: &Expr,
                              rs_ty: Ty<'tcx>,
                              rest: TypeRestrictions) -> Result<String, String> {
    Ok(match rs_ty.sty {
        ty_bool => format!("int8_t"),

        ty_int(TyIs) => format!("intptr_t"),
        ty_int(TyI8) => format!("int8_t"),
        ty_int(TyI16) => format!("int16_t"),
        ty_int(TyI32) => format!("int32_t"),
        ty_int(TyI64) => format!("int64_t"),

        ty_uint(TyUs) => format!("uintptr_t"),
        ty_uint(TyU8) => format!("uint8_t"),
        ty_uint(TyU16) => format!("uint16_t"),
        ty_uint(TyU32) => format!("uint32_t"),
        ty_uint(TyU64) => format!("uint64_t"),

        ty_float(TyF32) => format!("float"),
        ty_float(TyF64) => format!("double"),

        ty_ptr(mt { ref ty, .. }) |
        ty_rptr(_, mt { ref ty, .. }) |
        ty_uniq(ref ty) => {
            // We need to know if the type is Sized.
            // !Sized pointers are twice as wide as Sized pointers.
            if type_is_sized(&ParameterEnvironment::for_item(tcx, expr.id),
                             expr.span, ty) {
                // If it is a sized type, then the width of the type is the same as a pointer.
                // So we can treat it like a C++ raw pointer.
                format!("{}*", try!(cpp_type_of_internal(td, tcx, expr, ty,
                                                         TypeRestrictions::Pointer)))
            } else {
                // It's a trait object or slice!
                match ty.sty {
                    ty_str => format!("rs::StrSlice"),
                    ty_vec(ref it_ty, None) => {
                        format!("rs::Slice<{}>",
                                try!(cpp_type_of_internal(td, tcx, expr, it_ty,
                                                          TypeRestrictions::Slice)))
                    }

                    // Unsized types which aren't slices are trait objects of some type.
                    // We don't want to go out of our way to support them,
                    // but we return the correct width of pointer to keep layout correct.
                    _ => format!("rs::TraitObject"),
                }
            }
        }

        ty_enum(defid, _) => {
            if type_is_c_like_enum(tcx, rs_ty) {
                let repr_hints = lookup_repr_hints(tcx, defid);

                // Ensure that there is exactly 1 item in repr_hints
                if repr_hints.len() == 0 {
                    return td.void_or_dummy(rest).map_err(|_| {
                        format!("Enum type {} does not have a #[repr(_)] annotation. \
                                 Consider annotating it with #[repr(C)].", rs_ty.repr(tcx))
                    });
                } else if repr_hints.len() > 1 {
                    return td.void_or_dummy(rest).map_err(|_| {
                        format!("Enum type {} has multiple #[repr(_)] annotations.",
                                rs_ty.repr(tcx))
                    });
                }

                let mut defn = format!("enum class {}", with_path(tcx, defid, |segs| {
                    segs.last().unwrap()
                }).name().as_str());

                // Determine the representation of the enum class
                match repr_hints[0] {
                    ReprExtern => {
                        // #[repr(C)] => representation is the default!
                    }
                    ReprInt(_, ity) => {
                        // #[repr(int_type)] => representation is the int_type!
                        let repr = match ity {
                            SignedInt(ast::TyI8) => "int8_t",
                            UnsignedInt(ast::TyU8) => "uint8_t",
                            SignedInt(ast::TyI16) => "int16_t",
                            UnsignedInt(ast::TyU16) => "uint16_t",
                            SignedInt(ast::TyI32) => "int32_t",
                            UnsignedInt(ast::TyU32) => "uint32_t",
                            SignedInt(ast::TyI64) => "int64_t",
                            UnsignedInt(ast::TyU64) => "uint64_t",
                            SignedInt(ast::TyIs) => "intptr_t",
                            UnsignedInt(ast::TyUs) => "uintptr_t",
                        };

                        defn.push_str(&format!(" : {}", repr));
                    }
                    _ => {
                        return td.void_or_dummy(rest).map_err(|_| {
                            format!("Enum type {} has an unsupported #[repr(_)] annotation",
                                    rs_ty.repr(tcx))
                        });
                    }
                }

                defn.push_str(" {\n");

                let variants = enum_variants(tcx, defid);
                for variant in &*variants {
                    defn.push_str(&format!("    {} = {},\n",
                                           variant.name.as_str(), variant.disr_val));
                }

                defn.push_str("};");

                let name = with_path(tcx, defid, |segs| {
                    segs.fold(String::new(), |acc, new| {
                        if acc.is_empty() {
                            format!("rs::{}", new)
                        } else {
                            format!("{}::{}", acc, new)
                        }
                    })
                });

                td.add_type(name, None, Some(defn), Vec::new(), Vec::new())
            } else {
                return td.void_or_dummy(rest).map_err(|_| {
                    format!("Non C-like enum types like {} are not supported", rs_ty.repr(tcx))
                });
            }
        }

        ty_struct(defid, _) => {
            let repr_hints = lookup_repr_hints(tcx, defid);

            // Ensure that there is exactly 1 item in repr_hints
            if repr_hints.len() == 0 {
                return td.void_or_dummy(rest).map_err(|_| {
                    format!("Struct type {} does not have a #[repr(_)] annotation. \
                             Consider annotating it with #[repr(C)].", rs_ty.repr(tcx))
                });
            } else if repr_hints.len() > 1 {
                return td.void_or_dummy(rest).map_err(|_| {
                    format!("Struct type {} has multiple #[repr(_)] annotations.",
                            rs_ty.repr(tcx))
                });
            }

            let mut defn = format!("struct {}", with_path(tcx, defid, |segs| {
                segs.last().unwrap()
            }).name().as_str());
            let decl = format!("{};", defn);

            // Confirm that the struct is #[repr(C)]
            match repr_hints[0] {
                ReprExtern => {}
                _ => {
                    return td.void_or_dummy(rest).map_err(|_| {
                        format!("Struct type {} has an unsupported #[repr(_)] annotation",
                                rs_ty.repr(tcx))
                    });
                }
            }

            let fields = lookup_struct_fields(tcx, defid);

            defn.push_str(" {\n");
            defn.push_str("};");

            return td.void_or_dummy(rest).map_err(|_| {
                format!("Struct type {} is not ffi safe. Consider annotating it with #[repr(C)].", rs_ty.repr(tcx))
            });
        }

        // Unsupported types
        _ => {
            return td.void_or_dummy(rest).map_err(|_| {
                format!("Passing types like {} by-value between C++ and rust is not supported",
                        rs_ty.repr(tcx))
            });
        }
    })
}
