#![allow(dead_code)]
use std::collections::HashMap;
use std::pin::Pin;

use super::codegen::CodeGen;
use super::func::FuncGen;
use goscript_parser::ast::Ident;
use goscript_parser::errors::ErrorList;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::FileSet;
use goscript_types::{
    BasicType, Config, EntityType, ObjKey, PackageKey as TCPackageKey, TCObjects, TypeInfo,
};
use goscript_vm::null_key;
use goscript_vm::opcode::*;
use goscript_vm::value::*;
use goscript_vm::vm::ByteCode;

pub struct EntryGen<'a> {
    objects: Pin<Box<VMObjects>>,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    packages: Vec<PackageKey>,
    // pkg_indices maps TCPackageKey to the index (in the generated code) of the package
    pkg_indices: HashMap<TCPackageKey, OpIndex>,
    blank_ident: IdentKey,
}

impl<'a> EntryGen<'a> {
    pub fn new(asto: &'a AstObjects, tco: &'a TCObjects, bk: IdentKey) -> EntryGen<'a> {
        EntryGen {
            objects: Box::pin(VMObjects::new()),
            ast_objs: asto,
            tc_objs: tco,
            packages: Vec::new(),
            pkg_indices: HashMap::new(),
            blank_ident: bk,
        }
    }

    // generate the entry function for ByteCode
    fn gen_entry_func(&mut self, main_idx: OpIndex) -> FunctionKey {
        // import the 0th pkg and call the main function of the pkg
        let ftype = self.objects.default_closure_type.unwrap();
        let fkey = *GosValue::new_function(
            null_key!(),
            *ftype.as_type(),
            false,
            false,
            &mut self.objects,
        )
        .as_function();
        let func = &mut self.objects.functions[fkey];
        func.emit_import(main_idx);
        func.emit_code(Opcode::PUSH_IMM);
        // negative index for main func
        func.emit_data(-1);
        func.emit_load_field();
        func.emit_pre_call();
        func.emit_call(false);
        func.emit_return();
        fkey
    }

    pub fn gen(
        mut self,
        checker_result: &HashMap<TCPackageKey, TypeInfo>,
        main_pkg: TCPackageKey,
    ) -> ByteCode {
        let mut main_pkg_idx = None;
        for (&tcpkg, ti) in checker_result.iter() {
            // create vm packages and store the indices
            let name = self.tc_objs.pkgs[tcpkg].name().clone().unwrap();
            let pkey = self.objects.packages.insert(PackageVal::new(name));
            self.packages.push(pkey);
            let index = (self.packages.len() - 1) as OpIndex;
            self.pkg_indices.insert(tcpkg, index);
            if tcpkg == main_pkg {
                main_pkg_idx = Some(index);
            }

            CodeGen::new(
                &mut self.objects,
                self.ast_objs,
                self.tc_objs,
                ti,
                pkey,
                self.blank_ident,
            )
            .gen_with_files(&ti.ast_files, index);
        }

        let entry = self.gen_entry_func(main_pkg_idx.unwrap());
        ByteCode {
            objects: self.objects,
            packages: self.packages,
            entry: entry,
        }
    }
}

pub fn parse_check_gen(
    path: &str,
    config: &Config,
    fset: &mut FileSet,
    el: &ErrorList,
) -> Result<ByteCode, usize> {
    let asto = &mut AstObjects::new();
    let tco = &mut goscript_types::TCObjects::new();
    let results = &mut HashMap::new();
    let pkgs = &mut HashMap::new();

    let importer =
        &mut goscript_types::Importer::new(&config, fset, pkgs, results, asto, tco, el, 0);
    let key = goscript_types::ImportKey::new(path, "./");
    let main_pkg = importer.import(&key);

    if el.len() > 0 {
        Err(el.len())
    } else {
        let blank_ident = asto.idents.insert(Ident::blank(0));
        let gen = EntryGen::new(asto, tco, blank_ident);
        Ok(gen.gen(results, main_pkg.unwrap()))
    }
}

// get GosValue from type checker's Obj
fn const_value(obj: ObjKey, tc_objs: &TCObjects, sobjs: &mut StringObjs) -> GosValue {
    let obj_val = &tc_objs.lobjs[obj];
    match obj_val.entity_type() {
        EntityType::Const(val) => {
            let typ = tc_objs.types[obj_val.typ().unwrap()]
                .try_as_basic()
                .unwrap()
                .typ();
            match typ {
                BasicType::Bool | BasicType::UntypedBool => GosValue::Bool(val.bool_as_bool()),
                BasicType::Int |
                BasicType::Int8|
                BasicType::Int16|
                BasicType::Int32|
                BasicType::Rune| // = Int32
                BasicType::Int64|
                BasicType::Uint|
                BasicType::Uint8|
                BasicType::Byte| // = Uint8
                BasicType::Uint16|
                BasicType::Uint32|
                BasicType::Uint64|
                BasicType::Uintptr|
                BasicType::UnsafePointer |
                BasicType::UntypedInt |
                BasicType::UntypedRune => {
                    let (i, _ ) = val.int_as_i64();
                    GosValue::Int(i as isize)
                }
                BasicType::Float32 |
                BasicType::Float64 |
                BasicType::UntypedFloat => {
                    let (f, _) = val.num_as_f64();
                    GosValue::Float64(*f)
                }
                BasicType::Str | BasicType::UntypedString => GosValue::new_str(val.str_as_string(), sobjs),
                _ => unreachable!(),
                //Complex64,  todo
                //Complex128, todo
            }
        }
        _ => unreachable!(),
    }
}
