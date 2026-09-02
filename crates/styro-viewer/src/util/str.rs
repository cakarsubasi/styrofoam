use std::{
    ffi::{CStr, CString},
    sync::Arc,
};

/// Shared null terminated string
pub type SStr = Arc<CStr>;

pub fn c_string(str: impl Into<Vec<u8>>) -> SStr {
    let string = CString::new(str).unwrap();
    string.into()
}
