use std::ptr::null_mut;

use bindings::{LIBOBS_API_MAJOR_VER, obs_module_t};

mod audio_capture;
mod audio_renderer;

// region OBS_DECLARE_MODULE

pub static mut OBS_MODULE_POINTER: *mut obs_module_t = null_mut();

#[no_mangle]
pub unsafe extern "C" fn obs_module_set_pointer(module: *mut obs_module_t) {
    OBS_MODULE_POINTER = module;
}

#[no_mangle]
pub unsafe extern "C" fn obs_current_module() -> *mut obs_module_t {
    OBS_MODULE_POINTER
}

#[no_mangle]
pub unsafe extern "C" fn obs_module_ver() -> u32 {
    LIBOBS_API_MAJOR_VER
}

// endregion

#[no_mangle]
pub unsafe extern "C" fn obs_module_load() -> bool {
    audio_capture::register();
    audio_renderer::register();
    true
}
