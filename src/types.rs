use std::collections::HashSet;
use std::mem;

use rustc_front::hir::Expr;
use syntax::ast::{self, MetaItemKind, LitKind};
use syntax::ast::NodeId;
use syntax::ast::IntTy::*;
use syntax::ast::UintTy::*;
use syntax::ast::FloatTy::*;
use syntax::attr::{SignedInt, UnsignedInt, ReprExtern, ReprInt};
use syntax::codemap::Span;

use rustc::middle::ty::*;
use rustc::middle::def_id::DefId;
use rustc::lint::{LintContext, LateContext, Level};

declare_lint!(pub BAD_CXX_TYPE, Warn, "Unable to translate type to C++");

struct DeferredStruct {
    defid: DefId,
    nid: (NodeId, Span),
}

impl DeferredStruct {
    fn run(self, td: &mut TypeData, tcx: &TyCtxt) -> TypeName {
        let struct_def = tcx.lookup_adt_def(self.defid);

        let (ns_before, ns_after, name, path) = explode_path(tcx, self.defid);
        let mut tn = TypeName::new(path);

        let mut defn = format!("struct {} {{\n", &name);
        for fdd in struct_def.all_fields() {
            let ty = cpp_type_of_internal(td, tcx, self.nid, fdd.unsubst_ty(), false);

            // Add the field, merging any errors into our TypeName
            defn.push_str(&format!("    {} {};\n", &tn.merge(ty), &fdd.name));
        }
        defn.push_str("};");

        // If we have any errors, we shouldn't write out our declaration,
        // as it won't be valid.
        if tn.err.len() == 0 {
            td.decls.push_str(&format!("{}\n{}\n{}", ns_before, defn, ns_after));
        }

        tn
    }
}

pub struct TypeData {
    decls: String,
    queue: Vec<DeferredStruct>,
    declared: HashSet<String>,
}

impl TypeData {
    pub fn new() -> TypeData {
        TypeData {
            decls: String::new(),
            queue: Vec::new(),
            declared: HashSet::new(),
        }
    }

    pub fn to_cpp(&mut self, cx: &LateContext) -> String {
        while self.queue.len() != 0 {
            let mut todo = Vec::new();
            mem::swap(&mut todo, &mut self.queue);

            // Run each of the callbacks, and report any errors
            for cb in todo {
                let mut tn = cb.run(self, cx.tcx);
                tn.recover();
                tn.into_name(cx);
            }
        }
        // XXX Process queue here
        self.decls.clone()
    }
}

#[derive(Debug, Clone)]
struct TypeNameNote {
    msg: String,
    span: Option<Span>,
}

#[derive(Debug, Clone)]
struct TypeNameProblem {
    msg: String,
    span: Option<Span>,
    notes: Vec<TypeNameNote>,
}

#[derive(Debug, Clone)]
pub struct TypeName {
    name: String,
    warn: Vec<TypeNameProblem>,
    err: Vec<TypeNameProblem>,
}

impl TypeName {
    fn new(s: String) -> TypeName {
        TypeName {
            name: s,
            warn: Vec::new(),
            err: Vec::new(),
        }
    }

    fn from_str(s: &str) -> TypeName {
        TypeName::new(format!("{}", s))
    }

    fn error(err: String, span: Option<Span>) -> TypeName {
        TypeName {
            name: format!("rs::__Dummy"),
            warn: Vec::new(),
            err: vec![TypeNameProblem {
                          msg: err,
                          span: span,
                          notes: Vec::new(),
                      }],
        }
    }

    pub fn into_name(self, cx: &LateContext) -> String {
        if self.err.len() == 0 {
            for warn in &self.warn {
                if cx.current_level(BAD_CXX_TYPE) != Level::Allow {
                    if let Some(span) = warn.span {
                        cx.span_lint(BAD_CXX_TYPE, span, &warn.msg);
                    } else {
                        cx.lint(BAD_CXX_TYPE, &warn.msg);
                    }

                    cx.sess().note_without_error("C++ code will recieve an opaque reference");

                    for note in &warn.notes {
                        if let Some(span) = note.span {
                            cx.sess().span_note_without_error(span, &note.msg);
                        } else {
                            cx.sess().note_without_error(&note.msg);
                        }
                    }
                }
            }
        } else {
            for err in &self.err {
                if let Some(span) = err.span {
                    cx.sess().span_err(span, &err.msg);
                } else {
                    cx.sess().err(&err.msg);
                }

                cx.sess()
                  .note_without_error("This type can't be passed by value, and thus is an \
                                       invalid return type");

                for note in &err.notes {
                    if let Some(span) = note.span {
                        cx.sess().span_note_without_error(span, &note.msg);
                    } else {
                        cx.sess().note_without_error(&note.msg);
                    }
                }
            }

            // Don't bother reporting warnings if we are going to fail
            // Just report the errors
        }

        self.name
    }

    pub fn with_note(mut self, msg: String, span: Option<Span>) -> TypeName {
        for warn in &mut self.warn {
            warn.notes.push(TypeNameNote {
                msg: msg.clone(),
                span: span,
            });
        }
        for err in &mut self.err {
            err.notes.push(TypeNameNote {
                msg: msg.clone(),
                span: span,
            });
        }
        self
    }

    pub fn with_warn(mut self, msg: String, span: Option<Span>) -> TypeName {
        self.warn.push(TypeNameProblem {
            msg: msg,
            span: span,
            notes: Vec::new(),
        });
        self
    }

    pub fn with_name(mut self, name: String) -> TypeName {
        self.name = name;
        self
    }

    pub fn map_name<F>(self, f: F) -> TypeName
        where F: FnOnce(String) -> String
    {
        let TypeName{ name, warn, err } = self;
        let name = f(name);
        TypeName {
            name: name,
            warn: warn,
            err: err,
        }
    }

    pub fn merge(&mut self, other: TypeName) -> String {
        let TypeName{ name, warn, err } = other;

        for it in warn {
            self.warn.push(it)
        }
        for it in err {
            self.err.push(it)
        }
        name
    }

    /// This method is called when it is possible to require from
    /// typename generation problems. If there are any errors,
    /// they are converted into warnings, and true is returned.
    /// otherwise, false is returned.
    pub fn recover(&mut self) -> bool {
        if self.err.len() == 0 {
            return false;
        }

        let mut err = Vec::new();
        mem::swap(&mut self.err, &mut err);
        for e in err {
            self.warn.push(e);
        }

        true
    }
}

/// Takes the defid of a struct or enum, and produces useful strings for generating the C++ code
/// the results are as follows:
/// ns_before: The `namespace foo {` declarations which should be placed before the declaration
/// ns_after: The `}` declarations which should be placed after the declaration
/// name: The name of the struct or enum itself
/// path: The fully qualified path in C++ which the struct/enum should exist at
fn explode_path(tcx: &TyCtxt, defid: DefId) -> (String, String, String, String) {
    let mut ns_before = String::new();
    let mut ns_after = String::new();
    let mut name = String::new();
    let mut path = format!("::rs::");

    tcx.with_path(defid, |segs| {
        let mut segs_vec: Vec<_> = segs.map(|x| x.name()).collect();

        name = format!("{}", segs_vec.pop().unwrap().as_str());
        for seg in segs_vec {
            ns_before.push_str(&format!("namespace {} {{", seg.as_str()));
            ns_after.push_str("}\n");
            path.push_str(&format!("{}::", seg.as_str()));
        }
        path.push_str(&name);
    });

    (ns_before, ns_after, name, path)
}


fn cpp_type_attr_check(tcx: &TyCtxt, defid: DefId, rs_ty: Ty) -> Option<TypeName> {
    let attrs = tcx.get_attrs(defid);
    for attr in &*attrs {
        let metaitem = &attr.node.value.node;
        if let MetaItemKind::NameValue(ref name, ref value) = *metaitem {
            if *name == "cpp_type" {
                return if let LitKind::Str(ref s, _) = value.node {
                    Some(TypeName::from_str(s))
                } else {
                    // Error
                    Some(TypeName::error(format!("Struct type {} has a #[cpp_type] annotation, \
                                                  but it's value isn't a string",
                                                 rs_ty),
                                         None))
                };
            }
        }
    }
    None
}

/// Determine the c++ type for a given expression
/// This is the entry point for the types module, and is the intended mechanism for
/// invoking the type translation infrastructure.
pub fn cpp_type_of<'tcx>(td: &mut TypeData,
                         tcx: &TyCtxt<'tcx>,
                         expr: &Expr,
                         is_arg: bool)
                         -> TypeName {
    // Get the type object
    let rs_ty = tcx.expr_ty(expr);

    if !is_arg {
        // Special case for void return value
        if let TyTuple(ref it) = rs_ty.sty {
            if it.len() == 0 {
                return TypeName::from_str("void");
            }
        }
    }

    cpp_type_of_internal(td, tcx, (expr.id, expr.span), rs_ty, is_arg)
}

fn cpp_type_of_internal<'tcx>(td: &mut TypeData,
                              tcx: &TyCtxt<'tcx>,
                              nid: (NodeId, Span),
                              rs_ty: Ty<'tcx>,
                              in_ptr: bool)
                              -> TypeName {
    match rs_ty.sty {
        TyBool => TypeName::from_str("::rs::bool_"),

        TyInt(Is) => TypeName::from_str("::rs::isize"),
        TyInt(I8) => TypeName::from_str("::rs::i8"),
        TyInt(I16) => TypeName::from_str("::rs::i16"),
        TyInt(I32) => TypeName::from_str("::rs::i32"),
        TyInt(I64) => TypeName::from_str("::rs::i64"),

        TyUint(Us) => TypeName::from_str("::rs::usize"),
        TyUint(U8) => TypeName::from_str("::rs::u8"),
        TyUint(U16) => TypeName::from_str("::rs::u16"),
        TyUint(U32) => TypeName::from_str("::rs::u32"),
        TyUint(U64) => TypeName::from_str("::rs::u64"),

        TyFloat(F32) => TypeName::from_str("::rs::f32"),
        TyFloat(F64) => TypeName::from_str("::rs::f64"),

        TyRawPtr(TypeAndMut { ref ty, .. }) |
        TyRef(_, TypeAndMut { ref ty, .. }) |
        TyBox(ref ty) => {
            // We need to know if the type is Sized.
            // !Sized pointers are twice as wide as Sized pointers.
            if ty.is_sized(&ParameterEnvironment::for_item(tcx, nid.0), nid.1) {
                // We try to get the internal type - if that doesn't work out it's OK
                let mut cpp_ty = cpp_type_of_internal(td, tcx, nid, ty, true);

                // If we had a problem generating the type, make the errors warnings,
                // and emit the type void*
                if cpp_ty.recover() {
                    cpp_ty.with_name(format!("void*"))
                } else {
                    cpp_ty.map_name(|name| format!("{}*", &name))
                }
            } else {
                // It's a trait object or slice!
                match ty.sty {
                    TyStr => TypeName::from_str("::rs::StrSlice"),
                    TySlice(ref it_ty) => {
                        let mut cpp_ty = cpp_type_of_internal(td, tcx, nid, it_ty, true);

                        if cpp_ty.recover() {
                            cpp_ty.with_name(format!("::rs::Slice<void>"))
                        } else {
                            cpp_ty.map_name(|name| format!("::rs::Slice< {} >", &name))
                        }
                    }

                    // Unsized types which aren't slices are trait objects of some type.
                    // We don't want to go out of our way to support them,
                    // but we return the correct width of pointer to keep layout correct.
                    _ => {
                        TypeName::from_str("::rs::TraitObject")
                            .with_warn(format!("Type {} is an unsized type which cannot \
                                                currently be translated to C++",
                                               ty),
                                       None)
                    }
                }
            }
        }

        TyEnum(edef, _) => {
            let defid = edef.did;
            if let Some(tn) = cpp_type_attr_check(tcx, defid, rs_ty) {
                return tn;
            }

            if edef.is_payloadfree() {
                let repr_hints = tcx.lookup_repr_hints(defid);

                // Ensure that there is exactly 1 item in repr_hints
                if repr_hints.len() == 0 {
                    return TypeName::error(format!("Enum type {} does not have a #[repr(_)] \
                                                    annotation.",
                                                   rs_ty),
                                           None)
                               .with_note(format!("Consider annotating it with #[repr(C)]"), None);
                } else if repr_hints.len() > 1 {
                    return TypeName::error(format!("Enum type {} has multiple #[repr(_)] \
                                                    annotations",
                                                   rs_ty),
                                           None);
                }

                let (ns_before, ns_after, name, path) = explode_path(tcx, defid);
                let tn = TypeName::new(path);
                if td.declared.contains(&name) {
                    return tn;
                }

                let mut defn = format!("enum class {}", &name);

                // Determine the representation of the enum class
                match repr_hints[0] {
                    ReprExtern => {
                        // #[repr(C)] => representation is the default!
                    }
                    ReprInt(_, ity) => {
                        // #[repr(int_type)] => representation is the int_type!
                        let repr = match ity {
                            SignedInt(ast::IntTy::I8) => "::rs::i8",
                            UnsignedInt(ast::UintTy::U8) => "::rs::u8",
                            SignedInt(ast::IntTy::I16) => "::rs::i16",
                            UnsignedInt(ast::UintTy::U16) => "::rs::u16",
                            SignedInt(ast::IntTy::I32) => "::rs::i32",
                            UnsignedInt(ast::UintTy::U32) => "::rs::u32",
                            SignedInt(ast::IntTy::I64) => "::rs::i64",
                            UnsignedInt(ast::UintTy::U64) => "::rs::u64",
                            SignedInt(ast::IntTy::Is) => "::rs::isize",
                            UnsignedInt(ast::UintTy::Us) => "::rs::usize",
                        };

                        defn.push_str(&format!(" : {}", repr));
                    }
                    _ => {
                        return TypeName::error(format!("Enum type {} has unsupported #[repr(_)] \
                                                        annotation",
                                                       rs_ty),
                                               None);
                    }
                }

                defn.push_str(" {\n");
                for variant in &edef.variants {
                    defn.push_str(&format!("    {} = {},\n",
                                           variant.name.as_str(),
                                           variant.disr_val));
                }
                defn.push_str("};");

                // Record the declaration, and define the type
                td.decls.push_str(&format!("{}\n{}\n{}", ns_before, defn, ns_after));
                td.declared.insert(name);
                tn
            } else {
                TypeName::error(format!("Enum type {} is not a C-like enum", rs_ty), None)
            }
        }

        TyStruct(sdef, substs) => {
            let defid = sdef.did;
            if let Some(tn) = cpp_type_attr_check(tcx, defid, rs_ty) {
                return tn;
            }

            let repr_hints = tcx.lookup_repr_hints(defid);

            // Ensure that there is exactly 1 item in repr_hints, and that it is #[repr(C)]
            if repr_hints.len() == 0 {
                return TypeName::error(format!("Struct type {} does not have a #[repr(_)] \
                                                annotation",
                                               rs_ty),
                                       None)
                           .with_note(format!("Consider annotating it with #[repr(C)]"), None);
            } else if repr_hints.len() > 1 {
                return TypeName::error(format!("Struct type {} has multiple #[repr(_)] \
                                                annotations",
                                               rs_ty),
                                       None);
            } else if repr_hints[0] != ReprExtern {
                return TypeName::error(format!("Struct type {} has an unsupported #[repr(_)] \
                                                annotation",
                                               rs_ty),
                                       None);
            }

            // We don't support structs with substitutions (generic type parameters)
            if !substs.types.is_empty() {
                return TypeName::error(format!("Struct type {} has generic type parameters, \
                                                which are not supported",
                                               rs_ty),
                                       None);
            }

            let (ns_before, ns_after, name, path) = explode_path(tcx, defid);
            let tn = TypeName::new(path);
            if td.declared.contains(&name) {
                return tn;
            }

            let deferred = DeferredStruct {
                defid: defid,
                nid: nid,
            };

            if in_ptr {
                td.decls.push_str(&format!("{}\nstruct {};\n{}", ns_before, &name, ns_after));
                td.queue.push(deferred);
            } else {
                deferred.run(td, tcx);
            }

            td.declared.insert(name);
            tn
        }

        // Unsupported types
        _ => {
            TypeName::error(format!("The type {} cannot be passed between C++ and rust", rs_ty),
                            None)
        }
    }
}
