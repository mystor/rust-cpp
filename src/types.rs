use std::collections::HashMap;

use syntax::ast::{self, Expr};
use syntax::ast::IntTy::*;
use syntax::ast::UintTy::*;
use syntax::ast::FloatTy::*;
use syntax::ast_map::PathElem;
use syntax::attr::*;

use rustc::util::ppaux::Repr;
use rustc::middle::ty::*;

pub struct CppTy {
    decl: String,
    defn: Option<String>
}

pub struct CppNs {
    tys: HashMap<String, CppTy>,
    children: HashMap<String, CppNs>,
}

impl CppNs {
    pub fn decls_to_cpp(&self) -> String {
        let mut s = String::new();
        for (ref name, ref ns) in &self.children {
            s.push_str(&format!("namespace {} {{\n", name));
            s.push_str(&ns.decls_to_cpp());
            s.push_str(&format!("}} // namespace {}\n", name));
        }

        for ty in self.tys.values() {
            s.push_str(&ty.decl);
        }

        s
    }

    pub fn defns_to_cpp(&self) -> String {
        let mut s = String::new();
        for (ref name, ref ns) in &self.children {
            s.push_str(&format!("namespace {} {{\n", name));
            s.push_str(&ns.defns_to_cpp());
            s.push_str(&format!("}} // namespace {}\n", name));
        }

        for ty in self.tys.values() {
            match ty.defn {
                Some(ref dn) => s.push_str(dn),
                None => {}
            }
        }

        s
    }

    fn insert_type_at_path<I: Iterator<Item=PathElem>>(&mut self, ty: CppTy,
                                                       mut path: I) -> Option<CppTy> {
        match path.next() {
            Some(seg) => {
                let name = format!("{}", seg.name().as_str());


                if let Some(ty) = {
                    let child = self.children.entry(name.clone())
                        .or_insert_with(|| CppNs {
                            tys: HashMap::new(),
                            children: HashMap::new(),
                        });
                    child.insert_type_at_path(ty, path)
                } {
                    // Remove the namespace
                    assert!(self.children[&name].tys.is_empty());
                    assert!(self.children[&name].children.is_empty());

                    self.children.remove(&name);
                    self.tys.insert(name, ty);
                }

                None
            }
            None => Some(ty), // This signals to the caller to insert at this level
        }
    }
}

pub struct TypeData {
    root_ns: CppNs,
    dummy_count: usize,
}

impl TypeData {
    pub fn new() -> TypeData {
        TypeData {
            root_ns: CppNs {
                tys: HashMap::new(),
                children: HashMap::new(),
            },
            dummy_count: 0,
        }
    }

    pub fn to_cpp(&self) -> String {
        let mut s = String::new();

        s.push_str("namespace rs {\n");
        s.push_str("/* Forward Declarations */\n");
        s.push_str(&self.root_ns.decls_to_cpp());

        s.push_str("\n/* Definitions */\n");
        s.push_str(&self.root_ns.defns_to_cpp());
        s.push_str("} // namespace rs\n");

        s
    }

    fn void_or_dummy<'tcx>(&mut self, rest: TypeRestrictions) -> Result<String, ()> {
        if rest.can_be_void() {
            Ok(format!("void"))
        } else if rest.incomplete_type_ok() {
            let name = format!("__DummyType_{}", self.dummy_count);
            self.dummy_count += 1;

            self.root_ns.tys.insert(name.clone(), CppTy {
                decl: format!("class {};\n", &name),
                defn: None,
            });

            // The dummy type will be located in the rs:: module
            Ok(format!("rs::{}", name))
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

                let segs: Vec<_> = with_path(tcx, defid, |segs| {
                    segs.map(|seg| format!("{}", seg.name().as_str())).collect()
                });

                let mut defn = {
                    let name = segs.last().unwrap();
                    format!("enum class {}", &name)
                };

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

                defn.push_str("};\n");

                with_path(tcx, defid, |segs| {
                    td.root_ns.insert_type_at_path(CppTy {
                        // We use an empty string, as forward declarations of C++ enums
                        // are illegal.
                        decl: String::new(),
                        defn: Some(defn),
                    }, segs);
                });

                segs.into_iter().fold(String::new(), |acc, new| {
                    if acc.is_empty() {
                        format!("rs::{}", new)
                    } else {
                        format!("{}::{}", acc, new)
                    }
                })
            } else {
                return td.void_or_dummy(rest).map_err(|_| {
                    format!("Non C-like enum types like {} are not supported", rs_ty.repr(tcx))
                });
            }
        }

        ty_struct(defid, _) => {
            return td.void_or_dummy(rest).map_err(|_| {
                format!("Struct type {} is not ffi safe. Consider annotating it with #[repr(C)].", rs_ty.repr(tcx))
            });
            /* lookup_struct_fields(tcx, defid);
            if is_ffi_safe(tcx, ty) {
                ty
            } else {
                return td.void_or_dummy(rest).map_err(|_| {
                    format!("Struct type {} is not ffi safe. Consider annotating it with #[repr(C)].", rs_ty.repr(tcx))
                });
            } */
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
