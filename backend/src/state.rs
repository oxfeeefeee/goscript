use std::collections::HashMap;
use super::types::*;


pub struct Global {

}

pub struct State<'a> {
    g:          &'static Global,
    parent:     Option<&'a State<'a>>,
    env:        HashMap<GosValue, GosValue>,

    
}