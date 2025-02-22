use convert_case::{Case, Casing};
use rustler::types::atom::nil;
use rustler::types::tuple::{self, make_tuple};
use rustler::{Encoder, Error, Term, TermType};
use std::collections::HashMap;
use wasmtime::component::{Type, Val};
use wit_parser::{Resolve, Type as WitType, TypeDef, TypeDefKind};

use crate::atoms;

pub fn term_to_val(param_term: &Term, param_type: &Type) -> Result<Val, Error> {
    let term_type = param_term.get_type();
    match (term_type, param_type) {
        (TermType::Binary, Type::String) => Ok(Val::String(param_term.decode::<String>()?)),
        (TermType::Integer, Type::U8) => Ok(Val::U8(param_term.decode::<u8>()?)),
        (TermType::Integer, Type::U16) => Ok(Val::U16(param_term.decode::<u16>()?)),
        (TermType::Integer, Type::U64) => Ok(Val::U64(param_term.decode::<u64>()?)),
        (TermType::Integer, Type::U32) => Ok(Val::U32(param_term.decode::<u32>()?)),
        (TermType::Integer, Type::S8) => Ok(Val::S8(param_term.decode::<i8>()?)),
        (TermType::Integer, Type::S16) => Ok(Val::S16(param_term.decode::<i16>()?)),
        (TermType::Integer, Type::S64) => Ok(Val::S64(param_term.decode::<i64>()?)),
        (TermType::Integer, Type::S32) => Ok(Val::S32(param_term.decode::<i32>()?)),
        (TermType::Float, Type::Float32) => Ok(Val::Float32(param_term.decode::<f32>()?)),
        (TermType::Float, Type::Float64) => Ok(Val::Float64(param_term.decode::<f64>()?)),
        (TermType::Atom, Type::Bool) => Ok(Val::Bool(param_term.decode::<bool>()?)),
        (TermType::List, Type::List(list)) => {
            let decoded_list = param_term.decode::<Vec<Term>>()?;
            let list_values = decoded_list
                .iter()
                .map(|term| term_to_val(term, &list.ty()).unwrap())
                .collect::<Vec<Val>>();
            Ok(Val::List(list_values))
        }
        (TermType::Tuple, Type::Tuple(tuple)) => {
            let dedoded_tuple = tuple::get_tuple(*param_term)?;
            let tuple_types = tuple.types();
            let mut tuple_vals: Vec<Val> = Vec::with_capacity(tuple_types.len());
            for (tuple_type, tuple_term) in tuple_types.zip(dedoded_tuple) {
                let component_val = term_to_val(&tuple_term, &tuple_type)?;
                tuple_vals.push(component_val);
            }
            Ok(Val::Tuple(tuple_vals))
        }
        (TermType::Map, Type::Record(record)) => {
            let mut kv = Vec::with_capacity(record.fields().len());

            let decoded_map = param_term.decode::<HashMap<Term, Term>>()?;
            let terms = decoded_map
                .iter()
                .map(|(key_term, val)| (term_to_field_name(key_term), val))
                .collect::<Vec<(String, &Term)>>();
            for field in record.fields() {
                let field_term_option = terms.iter().find(|(k, _)| k == field.name);
                if let Some((_, field_term)) = field_term_option {
                    let field_value = term_to_val(field_term, &field.ty)?;
                    kv.push((field.name.to_string(), field_value))
                }
            }
            Ok(Val::Record(kv))
        }
        (TermType::Atom, Type::Option(option_type)) => {
            let the_atom = param_term.atom_to_string()?;
            if the_atom == "nil" {
                Ok(Val::Option(None))
            } else {
                let converted_val = term_to_val(param_term, &option_type.ty())?;
                Ok(Val::Option(Some(Box::new(converted_val))))
            }
        }
        (_term_type, Type::Option(option_type)) => {
            let converted_val = term_to_val(param_term, &option_type.ty())?;
            Ok(Val::Option(Some(Box::new(converted_val))))
        }
        (term_type, val_type) => Err(rustler::Error::Term(Box::new(format!(
            "Could not convert {:?} to {:?}",
            term_type, val_type
        )))),
    }
}

pub fn val_to_term<'a>(val: &Val, env: rustler::Env<'a>) -> Term<'a> {
    match val {
        Val::String(string) => string.encode(env),
        Val::Bool(bool) => bool.encode(env),
        Val::U64(num) => num.encode(env),
        Val::U32(num) => num.encode(env),
        Val::U16(num) => num.encode(env),
        Val::U8(num) => num.encode(env),
        Val::S8(num) => num.encode(env),
        Val::S16(num) => num.encode(env),
        Val::S32(num) => num.encode(env),
        Val::S64(num) => num.encode(env),
        Val::Float32(num) => num.encode(env),
        Val::Float64(num) => num.encode(env),
        Val::List(list) => list
            .iter()
            .map(|val| val_to_term(val, env))
            .collect::<Vec<Term<'a>>>()
            .encode(env),
        Val::Record(record) => {
            let converted_pairs = record
                .iter()
                .map(|(key, val)| (field_name_to_term(&env, key), val_to_term(val, env)))
                .collect::<Vec<(Term, Term)>>();
            Term::map_from_pairs(env, converted_pairs.as_slice()).unwrap()
        }
        Val::Tuple(tuple) => {
            let tuple_terms = tuple
                .iter()
                .map(|val| val_to_term(val, env))
                .collect::<Vec<Term<'a>>>();
            make_tuple(env, tuple_terms.as_slice())
        }
        Val::Option(option) => match option {
            Some(boxed_val) => val_to_term(boxed_val, env),
            None => nil().encode(env),
        },
        Val::Result(res) => {
            let ok: Term = if res.is_ok() {
                atoms::ok()
            } else {
                atoms::error()
            }
            .encode(env);

            let val = match res {
                Ok(Some(val)) => val.clone(),
                Err(Some(val)) => val.clone(),
                _ => return String::from("wut").encode(env),
            };
            let val = val_to_term(val.as_ref(), env);

            make_tuple(env, &[ok, val])
        }

        _ => String::from("wut").encode(env),
    }
}

pub fn vals_to_terms<'a>(vals: &[Val], env: rustler::Env<'a>) -> Vec<Term<'a>> {
    vals.iter()
        .map(|val| val_to_term(val, env))
        .collect::<Vec<Term<'a>>>()
}

pub fn term_to_field_name(key_term: &Term) -> String {
    match key_term.get_type() {
        TermType::Atom => key_term.atom_to_string().unwrap().to_case(Case::Kebab),
        _ => key_term.decode::<String>().unwrap().to_case(Case::Kebab),
    }
}

pub fn field_name_to_term<'a>(env: &rustler::Env<'a>, field_name: &str) -> Term<'a> {
    rustler::serde::atoms::str_to_term(env, &field_name.to_case(Case::Snake)).unwrap()
}

pub fn convert_params(param_types: &[Type], param_terms: Vec<Term>) -> Result<Vec<Val>, Error> {
    let mut params = Vec::with_capacity(param_types.len());

    for (param_term, param_type) in param_terms.iter().zip(param_types.iter()) {
        let param = term_to_val(param_term, param_type)?;
        params.push(param);
    }
    Ok(params)
}

pub fn encode_result<'a>(env: &rustler::Env<'a>, vals: Vec<Val>, from: Term<'a>) -> Term<'a> {
    let result_term = match vals.len() {
        1 => val_to_term(vals.first().unwrap(), *env),
        _ => vals
            .iter()
            .map(|term| val_to_term(term, *env))
            .collect::<Vec<Term>>()
            .encode(*env),
    };
    make_tuple(
        *env,
        &[
            atoms::returned_function_call().encode(*env),
            make_tuple(*env, &[atoms::ok().encode(*env), result_term]),
            from,
        ],
    )
}

pub fn convert_result_term(
    result_term: Term,
    wit_type: &WitType,
    wit_resolver: &Resolve,
) -> Result<Val, Error> {
    match wit_type {
        WitType::Bool => Ok(Val::Bool(result_term.decode::<bool>()?)),
        WitType::U8 => Ok(Val::U8(result_term.decode::<u8>()?)),
        WitType::U16 => Ok(Val::U16(result_term.decode::<u16>()?)),
        WitType::U32 => Ok(Val::U32(result_term.decode::<u32>()?)),
        WitType::U64 => Ok(Val::U64(result_term.decode::<u64>()?)),
        WitType::S8 => Ok(Val::S8(result_term.decode::<i8>()?)),
        WitType::S16 => Ok(Val::S16(result_term.decode::<i16>()?)),
        WitType::S32 => Ok(Val::S32(result_term.decode::<i32>()?)),
        WitType::S64 => Ok(Val::S64(result_term.decode::<i64>()?)),
        WitType::F32 => Ok(Val::Float32(result_term.decode::<f32>()?)),
        WitType::F64 => Ok(Val::Float64(result_term.decode::<f64>()?)),
        WitType::String => Ok(Val::String(result_term.decode::<String>()?)),
        WitType::Id(type_id) => {
            let complex_type = &wit_resolver.types[*type_id];

            convert_complex_result(result_term, complex_type, wit_resolver)
        }
        // You might want to handle other cases like Id, etc.
        _ => Err(Error::Term(Box::new("Unsupported type conversion"))),
    }
}

fn convert_complex_result(
    result_term: Term,
    complex_type: &TypeDef,
    wit_resolver: &Resolve,
) -> Result<Val, Error> {
    match &complex_type.kind {
        TypeDefKind::List(list_type) => {
            let decoded_list = result_term.decode::<Vec<Term>>()?;
            let list_values = decoded_list
                .iter()
                .map(|term| convert_result_term(*term, list_type, wit_resolver))
                .collect::<Result<Vec<Val>, Error>>()?;
            Ok(Val::List(list_values))
        }
        TypeDefKind::Record(record_type) => {
            let mut record_fields = Vec::with_capacity(record_type.fields.len());
            let decoded_map = result_term.decode::<HashMap<Term, Term>>()?;
            let field_term_tuples = decoded_map
                .iter()
                .map(|(key_term, val)| (term_to_field_name(key_term), val))
                .collect::<Vec<(String, &Term)>>();
            for field in &record_type.fields {
                let field_term_option = field_term_tuples.iter().find(|(k, _)| *k == field.name);
                if let Some((_, field_term)) = field_term_option {
                    let field_value = convert_result_term(**field_term, &field.ty, wit_resolver)?;
                    record_fields.push((field.name.to_string(), field_value))
                }
            }
            Ok(Val::Record(record_fields))
        }
        TypeDefKind::Tuple(tuple_type) => {
            let tuple_types = tuple_type.types.clone();
            let decoded_tuple = tuple::get_tuple(result_term)?;
            let mut tuple_vals: Vec<Val> = Vec::with_capacity(tuple_types.len());
            for (tuple_type, tuple_term) in tuple_types.iter().zip(decoded_tuple) {
                let component_val = convert_result_term(tuple_term, tuple_type, wit_resolver)?;
                tuple_vals.push(component_val);
            }
            Ok(Val::Tuple(tuple_vals))
        }
        _ => Err(Error::Term(Box::new("Unsupported type conversion"))),
    }
    // Type::String
}
