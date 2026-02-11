use std::ffi::CStr;
use std::os::raw::c_char;

#[unsafe(no_mangle)]
pub extern "C" fn helm_init(db_path: *const c_char) -> bool {
    if db_path.is_null() {
        return false;
    }

    // Placeholder implementation for now
    let c_str = unsafe { CStr::from_ptr(db_path) };
    match c_str.to_str() {
        Ok(path) => {
            println!("Initializing Helm with DB at: {}", path);
            true
        }
        Err(_) => false,
    }
}
