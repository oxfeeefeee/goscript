#![allow(dead_code)]
pub enum Type {}

#[derive(Copy, Clone, Debug)]
pub enum BasicType {
    Invalid,
    // predeclared types
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Uintptr,
    Float32,
    Float64,
    Complex64,
    Complex128,
    Str,
    UnsafePointer,
    // types for untyped values
    UntypedBool,
    UntypedInt,
    UntypedRune,
    UntypedFloat,
    UntypedComplex,
    UntypedString,
    UntypedNil,
    // aliases
    Byte, // = Uint8
    Rune, // = Int32
}

impl BasicType {
    pub fn is_unsigned(&self) -> bool {
        match self {
            BasicType::Uint
            | BasicType::Uint8
            | BasicType::Uint16
            | BasicType::Uint32
            | BasicType::Uint64
            | BasicType::Byte
            | BasicType::Rune
            | BasicType::Uintptr => true,
            _ => false,
        }
    }

    pub fn is_untyped(&self) -> bool {
        match self {
            BasicType::UntypedBool
            | BasicType::UntypedInt
            | BasicType::UntypedRune
            | BasicType::UntypedFloat
            | BasicType::UntypedComplex
            | BasicType::UntypedString
            | BasicType::UntypedNil => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum BasicInfo {
    IsInvalid,
    IsBoolean,
    IsInteger,
    IsFloat,
    IsComplex,
    IsString,
}

impl BasicInfo {
    pub fn is_ordered(&self) -> bool {
        match self {
            BasicInfo::IsInteger | BasicInfo::IsFloat | BasicInfo::IsString => true,
            _ => false,
        }
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            BasicInfo::IsInteger | BasicInfo::IsFloat | BasicInfo::IsComplex => true,
            _ => false,
        }
    }

    pub fn is_const_type(&self) -> bool {
        match self {
            BasicInfo::IsInteger
            | BasicInfo::IsFloat
            | BasicInfo::IsComplex
            | BasicInfo::IsString => true,
            _ => false,
        }
    }
}

/// A Basic represents a basic type.
#[derive(Copy, Clone, Debug)]
pub struct Basic {
    typ: BasicType,
    info: BasicInfo,
    name: &'static str,
}

impl Basic {
    pub fn typ(&self) -> BasicType {
        self.typ
    }

    pub fn info(&self) -> BasicInfo {
        self.info
    }

    pub fn name(&self) -> &str {
        self.name
    }
}

/// An Array represents an array type.
pub struct Array {
    len: i64,
    elem: Type,
}

impl Array {
    pub fn new(elem: Type, len: i64) -> Array {
        Array {
            len: len,
            elem: elem,
        }
    }

    pub fn len(&self) -> i64 {
        self.len
    }

    pub fn elem(&self) -> &Type {
        &self.elem
    }
}

/// A Slice represents a slice type.
pub struct Slice {
    elem: Type,
}

impl Slice {
    pub fn new(elem: Type) -> Slice {
        Slice { elem: elem }
    }

    pub fn elem(&self) -> &Type {
        &self.elem
    }
}

/// A Struct represents a struct type
pub struct Struct {
    //fields:
}
