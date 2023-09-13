use std::mem::size_of;

use bindings::{audio_output_get_channels, obs_audio_data, obs_data_t, obs_get_audio, obs_register_source_s, OBS_SOURCE_AUDIO, obs_source_info, obs_source_t, obs_source_type_OBS_SOURCE_TYPE_FILTER};

use crate::audio_renderer::dispatch;

pub static mut AUDIO_CAPTURE_LIST: Vec<*mut AudioCapture> = Vec::new();

pub struct AudioCapture {
    pub source: *mut obs_source_t,
    pub channels: usize,
}

pub unsafe fn register() {
    obs_register_source_s(&obs_source_info {
        id: "audio_capture\0".as_ptr().cast(),
        type_: obs_source_type_OBS_SOURCE_TYPE_FILTER,
        output_flags: OBS_SOURCE_AUDIO,
        get_name: Some(filter_getname),
        create: Some(filter_create),
        destroy: Some(filter_destroy),
        filter_audio: Some(filter_audio),
        ..Default::default()
    }, size_of::<obs_source_info>());
}

unsafe extern "C" fn filter_getname(_type_data: *mut ::std::os::raw::c_void) -> *const ::std::os::raw::c_char {
    "Audio Capture\0".as_ptr().cast()
}

unsafe extern "C" fn filter_create(_settings: *mut obs_data_t, source: *mut obs_source_t) -> *mut ::std::os::raw::c_void {
    let p = Box::into_raw(Box::new(AudioCapture {
        source,
        channels: audio_output_get_channels(obs_get_audio()),
    }));
    AUDIO_CAPTURE_LIST.push(p);
    p as _
}

unsafe extern "C" fn filter_destroy(data: *mut ::std::os::raw::c_void) {
    let p = data as *mut AudioCapture;
    AUDIO_CAPTURE_LIST.retain(|v| *v != p);
    let _ = Box::from_raw(p);
}

unsafe extern "C" fn filter_audio(data: *mut ::std::os::raw::c_void, audio: *mut obs_audio_data) -> *mut obs_audio_data {
    let audio_capture = &mut *(data as *mut AudioCapture);
    dispatch(audio_capture.source, audio, audio_capture.channels);
    audio
}
