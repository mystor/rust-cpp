use types::TypeData;

use syntax::codemap::{Span, FileLines};
use rustc::session::Session;

use std::sync::Mutex;
use std::collections::HashMap;

lazy_static! {
    pub static ref CPP_HEADERS: Mutex<String> = Mutex::new(String::new());
    pub static ref CPP_FNDECLS: Mutex<HashMap<String, CppFn>> = Mutex::new(HashMap::new());
    pub static ref CPP_TYPEDATA: Mutex<TypeData> = Mutex::new(TypeData::new());
    pub static ref CPP_TARGET: Mutex<String> = Mutex::new(String::new());
    pub static ref CPP_FLAGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

pub struct CppParam {
    pub mutable: bool,
    pub name: String,
    pub ty: Option<String>,
}

impl CppParam {
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        if !self.mutable {
            s.push_str("const ");
        }

        if let Some(ref ty) = self.ty {
            s.push_str(ty);
        } else {
            s.push_str("void");
        }
        s.push_str("& ");

        s.push_str(&self.name);

        s
    }
}

pub struct CppFn {
    pub name: String,
    pub arg_idents: Vec<CppParam>,
    pub ret_ty: Option<String>,
    pub body: String,
    pub span: Span,
}

impl CppFn {
    pub fn to_string(&self, sess: &Session) -> String {
        // Generate the parameter list
        let c_params = self.arg_idents.iter().fold(String::new(), |acc, new| {
            if acc.is_empty() {
                new.to_string()
            } else {
                format!("{}, {}", acc, new.to_string())
            }
        });

        let c_ty = if let Some(ref ty) = self.ret_ty {
            &ty[..]
        } else {
            panic!("Unexpected None ret_ty on CppFn")
        };

        let line = match sess.codemap().span_to_lines(self.span) {
            Ok(FileLines{file, lines}) => {
                if lines.len() < 1 {
                    String::new()
                } else {
                    format!("#line {} {:?}", lines[0].line_index + 1, file.name)
                }
            }
            Err(_) => String::new()
        };

        format!("{}\n{} {}({}) {}", line, c_ty, self.name, c_params, self.body)
    }
}
