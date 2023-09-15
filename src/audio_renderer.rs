use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::mem::size_of;
use std::ptr::{null_mut, slice_from_raw_parts};
use std::sync::Mutex;

use bindings::{blog, gs_color_format_GS_BGRA, gs_draw_sprite, GS_DYNAMIC, gs_effect_get_param_by_name, gs_effect_set_texture, gs_effect_t, gs_texture_create, gs_texture_destroy, gs_texture_set_image, gs_texture_t, LOG_ERROR, obs_audio_data, obs_combo_format_OBS_COMBO_FORMAT_STRING, obs_combo_type_OBS_COMBO_TYPE_LIST, obs_data_get_double, obs_data_get_int, obs_data_get_string, obs_data_set_default_double, obs_data_set_default_int, obs_data_t, obs_enter_graphics, obs_leave_graphics, obs_properties_add_float_slider, obs_properties_add_int, obs_properties_add_list, obs_properties_add_text, obs_properties_create, obs_properties_t, obs_property_list_add_string, obs_register_source_s, obs_source_get_name, obs_source_get_uuid, obs_source_info, obs_source_t, obs_source_type_OBS_SOURCE_TYPE_INPUT, OBS_SOURCE_VIDEO, obs_text_type_OBS_TEXT_INFO};

use crate::audio_capture::AUDIO_CAPTURE_LIST;

const MAX_AUDIO_SOURCE_COUNT: usize = 4;

pub static mut AUDIO_RENDERER_LIST: Mutex<Vec<*mut AudioRenderer>> = Mutex::new(Vec::new());

/// 将 `data` f32 数组使用 [`encode_audio_sample`] 编码为 BGRA 格式并填充到 `buf` u8 数组中
fn fill_texture_buffer(texture_buffer: &mut [u8], mut audio_buffer: impl Iterator<Item=f32>, width: usize, cell_width: usize, cell_height: usize, packet_index: usize) {
    let prefix = [
        packet_index & 0x1 != 0,
        packet_index & 0x2 != 0,
        packet_index & 0x4 != 0,
        packet_index & 0x8 != 0,
    ];
    let mut prefix_iter = prefix.into_iter();

    let height = texture_buffer.len() / 4 / width;
    let cell_pixel_count = cell_width * cell_height;
    for y in (0..height).step_by(cell_height) {
        for x in (0..width).step_by(cell_width) {
            if let Some(v ) = prefix_iter.next() { // buffer 前 4 个数据点是包序号，用于同步
                let gray = if v { 255u8 } else { 0u8 };
                for j in 0..cell_height {
                    for i in 0..cell_width {
                        let texture_buffer_index = 4 * ((y + j) * width + (x + i)); // 因为 buf 中的存储格式是 BGRX，所以需要乘以 4
                        texture_buffer[texture_buffer_index + 0] = gray; // B
                        texture_buffer[texture_buffer_index + 1] = gray; // G
                        texture_buffer[texture_buffer_index + 2] = gray; // R
                        texture_buffer[texture_buffer_index + 3] = 255; // A
                    }
                }
            } else if let Some(v) = audio_buffer.next() {
                // 音频部分
                // 音频数据编码到 16.0 ~ 256.0 范围
                let v1 = 16.0 + 120.0 * (v + 1.0);
                let mut n = (cell_pixel_count * 2) as f32;
                let mut v2 = v1 * n;
                for j in 0..cell_height {
                    for i in 0..cell_width {
                        // 在一个 cell 中，要进行 dithering，如果 cell_width cell_height 都是 2 的话，相当于 4 个像素编码 1 个采样，可以多 2bit 信息。
                        // RGB 按 1:2:1 分配
                        // R 和 B 取相同数值，G 取另一个数值，这样 1 个像素可以编码 2 个 8bit 信息，相当于 1 个像素编码了 9bit 的信息，如果一个 cell 是 4 个像素相当于 1 个音频采样编码成了 11bit 的深度。
                        let r3 = (if v1 * n >= v2 { v1 as i32 } else { v1 as i32 + 1 }).clamp(16, 255);
                        v2 -= r3 as f32;
                        n -= 1.0;
                        let g3 = (if v1 * n >= v2 { v1 as i32 } else { v1 as i32 + 1 }).clamp(16, 255);
                        v2 -= g3 as f32;
                        n -= 1.0;
                        let texture_buffer_index = 4 * ((y + j) * width + (x + i)); // 因为 buf 中的存储格式是 BGRX，所以需要乘以 4
                        texture_buffer[texture_buffer_index + 0] = r3 as u8; // B
                        texture_buffer[texture_buffer_index + 1] = g3 as u8; // G
                        texture_buffer[texture_buffer_index + 2] = r3 as u8; // R
                        texture_buffer[texture_buffer_index + 3] = 255; // A
                    }
                }
            } else {
                // 其余部分静音
                // 静音状态的颜色是纯黑色
                for j in 0..cell_height {
                    for i in 0..cell_width {
                        let texture_buffer_index = ((y + j) * width + (x + i)) * 4;
                        texture_buffer[texture_buffer_index + 0] = 0; // B
                        texture_buffer[texture_buffer_index + 1] = 0; // G
                        texture_buffer[texture_buffer_index + 2] = 0; // R
                        texture_buffer[texture_buffer_index + 3] = 255; // A
                    }
                }
            }
        }
    }
}

fn truncate_front<T>(q: &mut VecDeque<T>, n: usize) {
    for _ in 0..n {
        q.pop_front();
    }
}

fn mix_audio_buffer(audio_buffer: &mut VecDeque<f32>, audio_data: &[f32], amplifier: f32, mut source_sample_number: usize, base_sample_number: usize) -> usize {
    if source_sample_number < base_sample_number {
        source_sample_number = base_sample_number;
    } else if source_sample_number > base_sample_number + audio_buffer.len() {
        source_sample_number = base_sample_number + audio_buffer.len();
    }
    for v in audio_data {
        let v = *v * amplifier;
        if let Some(target) = audio_buffer.get_mut(source_sample_number - base_sample_number) {
            *target += v;
        } else {
            audio_buffer.push_back(v);
        }
        source_sample_number += 1;
    }
    source_sample_number
}

pub unsafe fn dispatch(audio_capture_source: *mut obs_source_t, audio: *mut obs_audio_data, channels: usize) {
    let audio_capture_uuid = CStr::from_ptr(obs_source_get_uuid(audio_capture_source));
    for audio_renderer in &*AUDIO_RENDERER_LIST.lock().unwrap() {
        let audio_renderer = &mut **audio_renderer;
        for (i, source_uuid) in audio_renderer.source_uuids.iter().enumerate() {
            if audio_capture_uuid == source_uuid.as_c_str() {
                let mut audio_buffer = audio_renderer.audio_buffer.lock().unwrap();
                // 仅支持双声道
                if channels >= 1 {
                    let base_sample_number = audio_renderer.base_sample_number;
                    let source_sample_number = audio_renderer.source_sample_number[i];
                    let source_amplifier = audio_renderer.source_amplifier[i];
                    let new_source_sample_number = if channels == 1 {
                        mix_audio_buffer(&mut audio_buffer[0], &*slice_from_raw_parts((*audio).data[0] as *mut f32, (*audio).frames as usize), source_amplifier, source_sample_number, base_sample_number);
                        mix_audio_buffer(&mut audio_buffer[1], &*slice_from_raw_parts((*audio).data[0] as *mut f32, (*audio).frames as usize), source_amplifier, source_sample_number, base_sample_number)
                    } else if channels >= 2 {
                        mix_audio_buffer(&mut audio_buffer[0], &*slice_from_raw_parts((*audio).data[0] as *mut f32, (*audio).frames as usize), source_amplifier, source_sample_number, base_sample_number);
                        mix_audio_buffer(&mut audio_buffer[1], &*slice_from_raw_parts((*audio).data[1] as *mut f32, (*audio).frames as usize), source_amplifier, source_sample_number, base_sample_number)
                    } else {
                        unreachable!();
                    };
                    audio_renderer.source_sample_number[i] = new_source_sample_number;
                }
            }
        }
    }
}

pub struct AudioRenderer {
    pub source_uuids: [CString; MAX_AUDIO_SOURCE_COUNT],
    pub source_amplifier: [f32; MAX_AUDIO_SOURCE_COUNT],
    pub width: usize,
    pub height: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    pub flush_len: usize,
    pub texture_buffer: Vec<u8>,
    pub texture: *mut gs_texture_t,
    /// audio_buffer 仅支持双声，支持最多 4 个源
    pub audio_buffer: Mutex<[VecDeque<f32>; 2]>,
    /// 音频源当前已写入 audio_buffer 的数据的下一个采样的序号
    ///
    /// 1. 如果 source_sample_number < base_sample_number 则表示当前源已停用，通常此时 source_sample_number == 0
    /// 2. 如果 source_sample_number == base_sample_number 则表示当前源数据的还未填充
    /// 3. 如果 source_sample_number > base_sample_number 则表示当前源数据已填充数据
    ///
    /// 如果某一源的 source_sample_number == base_sample_number，并且存在其他 source_sample_number - base_sample_number > 3 * flush_len
    /// 则认为此源已停用，此时将 source_sample_number 设为 0，不再参与混合输出
    ///
    /// 已填充数据的源会等待未填充数据的源最多 3 被 flush_len，如果再不填充，则忽略这个源
    ///
    /// 对于 source_sample_number < base_sample_number 的源，下一次填充数据时，自动从 base_sample_number 开始填充
    pub source_sample_number: [usize; MAX_AUDIO_SOURCE_COUNT],
    /// audio_buffer 第一个采样对应的采样序号
    pub base_sample_number: usize,
    /// 视频帧变化的次数，用作时钟
    pub packet_index: usize,
}

pub unsafe fn register() {
    obs_register_source_s(&obs_source_info {
        id: "audio_renderer\0".as_ptr().cast(),
        type_: obs_source_type_OBS_SOURCE_TYPE_INPUT,
        output_flags: OBS_SOURCE_VIDEO,
        get_name: Some(filter_getname),
        create: Some(filter_create),
        destroy: Some(filter_destroy),
        get_width: Some(get_width),
        get_height: Some(get_height),
        get_defaults: Some(get_defaults),
        get_properties: Some(get_properties),
        update: Some(filter_update),
        video_render: Some(video_render),
        ..Default::default()
    }, size_of::<obs_source_info>());
}

unsafe extern "C" fn filter_getname(_type_data: *mut ::std::os::raw::c_void) -> *const ::std::os::raw::c_char {
    "Audio Renderer\0".as_ptr().cast()
}

unsafe extern "C" fn filter_create(settings: *mut obs_data_t, _source: *mut obs_source_t) -> *mut ::std::os::raw::c_void {
    let audio_renderer = Box::new(AudioRenderer {
        source_uuids: Default::default(),
        source_amplifier: Default::default(),
        width: 0,
        height: 0,
        cell_width: 0,
        cell_height: 0,
        flush_len: 0,
        texture_buffer: Vec::new(),
        texture: null_mut(),
        audio_buffer: Default::default(),
        source_sample_number: Default::default(),
        base_sample_number: 1, // 0 用于默认值
        packet_index: 0,
    });
    let p = Box::into_raw(audio_renderer);
    filter_update(p as _, settings);
    AUDIO_RENDERER_LIST.lock().unwrap().push(p);
    p as _
}

unsafe extern "C" fn filter_destroy(data: *mut ::std::os::raw::c_void) {
    let p = data as *mut AudioRenderer;
    obs_enter_graphics();
    gs_texture_destroy((*p).texture);
    obs_leave_graphics();
    AUDIO_RENDERER_LIST.lock().unwrap().retain(|v| *v != p);
    let _ = Box::from_raw(p);
}

unsafe extern "C" fn get_width(data: *mut ::std::os::raw::c_void) -> u32 {
    let audio_renderer = &mut *(data as *mut AudioRenderer);
    audio_renderer.width as _
}

unsafe extern "C" fn get_height(data: *mut ::std::os::raw::c_void) -> u32 {
    let audio_renderer = &mut *(data as *mut AudioRenderer);
    audio_renderer.height as _
}

unsafe extern "C" fn get_defaults(settings: *mut obs_data_t) {
    for i in 0..MAX_AUDIO_SOURCE_COUNT {
        let _ = obs_data_set_default_double(settings, format!("source{}_amplifier\0", i).as_ptr().cast(), 1.0);
    }
    obs_data_set_default_int(settings, "width\0".as_ptr().cast(), 32);
    obs_data_set_default_int(settings, "height\0".as_ptr().cast(), 1072);
    obs_data_set_default_int(settings, "cell_width\0".as_ptr().cast(), 2);
    obs_data_set_default_int(settings, "cell_height\0".as_ptr().cast(), 2);
    obs_data_set_default_int(settings, "flush_len\0".as_ptr().cast(), 2400);
}

unsafe extern "C" fn get_properties(_data: *mut ::std::os::raw::c_void) -> *mut obs_properties_t {
    let props = obs_properties_create();
    for i in 0..MAX_AUDIO_SOURCE_COUNT {
        let list = obs_properties_add_list(props, format!("source{}\0", i).as_ptr().cast(), format!("声音源{}（必须为 Audio Capture 滤镜）\0", i + 1).as_ptr().cast(), obs_combo_type_OBS_COMBO_TYPE_LIST, obs_combo_format_OBS_COMBO_FORMAT_STRING);
        obs_property_list_add_string(list, "\0".as_ptr().cast(), "\0".as_ptr().cast());
        for audio_capture in &AUDIO_CAPTURE_LIST {
            let uuid = obs_source_get_uuid((**audio_capture).source);
            let name = obs_source_get_name((**audio_capture).source);
            obs_property_list_add_string(list, name, uuid);
        }
        let _ = obs_properties_add_float_slider(props, format!("source{}_amplifier\0", i).as_ptr().cast(), format!("声音源{}放大倍数\0", i + 1).as_ptr().cast(), 0.01, 10.00, 0.01);
    }
    let _ = obs_properties_add_int(props, "width\0".as_ptr().cast(), "编码区域宽度（单位：像素）（推荐为 32）\0".as_ptr().cast(), 1, 7680, 1);
    let _ = obs_properties_add_int(props, "height\0".as_ptr().cast(), "编码区域高度（单位：像素）（推荐为 1072）\0".as_ptr().cast(), 2, 4320, 2);
    let _ = obs_properties_add_int(props, "cell_width\0".as_ptr().cast(), "每个数据编码的格子宽度（单位：像素）（推荐为 2）\0".as_ptr().cast(), 1, 16, 1);
    let _ = obs_properties_add_int(props, "cell_height\0".as_ptr().cast(), "每个数据编码的格子高度（单位：像素）（推荐为 2）\0".as_ptr().cast(), 1, 16, 1);
    let _ = obs_properties_add_int(props, "flush_len\0".as_ptr().cast(), "最少缓冲长度（单位：采样）（推荐为 2400）\0".as_ptr().cast(), 480, 9600, 1);
    let _ = obs_properties_add_text(props, "help_1\0".as_ptr().cast(), "缓冲长度说明：如果按推荐设置的话，每个声道占用一半高度，每个声道是 32 * 1072 / 2 的画面区域，每个音频采样编码成 2x2 的格子，因此最多可以编码 (32 * 1072 / 2) / (2 * 2) = 4288 个采样。编码 2400 个采样对应 2400 / 48000 = 0.05s，因此编码区域大约每 3 帧画面会更新一次。同时，音频会比画面落后 0.05s。需要注意，这里并不一定恰好是 2400 个采样，如果声音源每批提交 512 采样的数据，那么声音源提交 5 批数据之后，画面上会显示 2560 个采样，这样的话画面会每 3 ~ 4 帧更新一次。\0".as_ptr().cast(), obs_text_type_OBS_TEXT_INFO);
    let _ = obs_properties_add_text(props, "help_2\0".as_ptr().cast(), "编码原理说明：在目标声音源上添加 Audio Capture 滤镜，这个滤镜负责获取声音数据。然后添加一个 Audio Renderer 的视频源，这个视频源负责将 Audio Capture 获取到的声音数据渲染成视频形式。它将音频采样信息转换成一系列明暗变化的点的图像信息。上半部分是左声道，下半部分是右声道。每个音频采样数据是 -1.0 ~ 1.0 的浮点数，他会被编码为 16 ~ 255 的灰度值，这样编码声音的位深大概是 8bit。如果每个格子为 2 x 2 = 4 个像素，那么位深可以增加到 10bit。由于视频压缩是有损的，实际上会损失一些精度，不过这样的音频听感基本上足够了。\0".as_ptr().cast(), obs_text_type_OBS_TEXT_INFO);
    let _ = obs_properties_add_text(props, "help_3\0".as_ptr().cast(), "多个声音源混合问题：由于声音混合的实现比较简单，如果声音源没有连续提交声音数据的话，会产生杂音。在声音源停止提供数据时会因为等待数据而卡住，并且之后因为追赶卡住的进度会故意丢帧，因此产生杂音。不过通常来自游戏的桌面声音、来自麦克风的声音、媒体源不会有这个问题。\0".as_ptr().cast(), obs_text_type_OBS_TEXT_INFO);
    let _ = obs_properties_add_text(props, "LICENSE\0".as_ptr().cast(), "本插件基于 GPLv2 开源。你可以在 https://github.com/ganlvtech/obs-audio-renderer 免费下载。\0".as_ptr().cast(), obs_text_type_OBS_TEXT_INFO);
    props
}

unsafe extern "C" fn filter_update(data: *mut ::std::os::raw::c_void, settings: *mut obs_data_t) {
    let audio_renderer = &mut *(data as *mut AudioRenderer);
    let width = obs_data_get_int(settings, "width\0".as_ptr().cast()) as usize;
    let height = obs_data_get_int(settings, "height\0".as_ptr().cast()) as usize;
    let cell_width = obs_data_get_int(settings, "cell_width\0".as_ptr().cast()) as usize;
    let cell_height = obs_data_get_int(settings, "cell_height\0".as_ptr().cast()) as usize;
    let flush_len = obs_data_get_int(settings, "flush_len\0".as_ptr().cast()) as usize;
    if width == 0 {
        return;
    }
    if height == 0 {
        return;
    }
    if cell_width == 0 {
        return;
    }
    if cell_height == 0 {
        return;
    }
    if height % 2 != 0 { // 必须是 2 的倍数
        blog(LOG_ERROR, format!("[audio_renderer] 编码区域高度必须是 2 的倍数\0").as_ptr().cast());
        return;
    }
    if width % cell_width != 0 { // 必须整除
        blog(LOG_ERROR, format!("[audio_renderer] 编码区域宽度必须整除格子宽度\0").as_ptr().cast());
        return;
    }
    if (height / 2) % cell_height != 0 { // 必须整除
        blog(LOG_ERROR, format!("[audio_renderer] 编码区域高度的一半必须整除格子高度\0").as_ptr().cast());
        return;
    }
    if (width * height / 2) / (cell_width * cell_height) < flush_len { // 编码区域不够大
        blog(LOG_ERROR, format!("[audio_renderer] 编码区域大小必须大于缓冲长度\0").as_ptr().cast());
        return;
    }
    for i in 0..MAX_AUDIO_SOURCE_COUNT {
        audio_renderer.source_uuids[i] = CString::from(CStr::from_ptr(obs_data_get_string(settings, format!("source{}\0", i).as_ptr().cast())));
        audio_renderer.source_amplifier[i] = obs_data_get_double(settings, format!("source{}_amplifier\0", i).as_ptr().cast()) as f32;
    }
    audio_renderer.width = width;
    audio_renderer.height = height;
    audio_renderer.cell_width = cell_width;
    audio_renderer.cell_height = cell_height;
    audio_renderer.flush_len = flush_len;
    let texture_buffer = vec![0u8; width * height * 4];
    obs_enter_graphics();
    if !audio_renderer.texture.is_null() {
        gs_texture_destroy(audio_renderer.texture);
    }
    audio_renderer.texture_buffer = texture_buffer;
    // GS_DYNAMIC 表示此 texture 会动态更新
    audio_renderer.texture = gs_texture_create(width as _, height as _, gs_color_format_GS_BGRA, 1, null_mut(), GS_DYNAMIC);
    obs_leave_graphics();
}

unsafe extern "C" fn video_render(data: *mut ::std::os::raw::c_void, effect: *mut gs_effect_t) {
    let audio_renderer = &mut *(data as *mut AudioRenderer);
    if audio_renderer.texture.is_null() {
        return;
    }

    let mut modified = false;
    {
        let mut audio_buffer = audio_renderer.audio_buffer.lock().unwrap();
        let base_sample_number = audio_renderer.base_sample_number;
        // 忽略落后 3 * flush_len 的源
        let max_source_sample_number = audio_renderer.source_sample_number.iter().map(|v| *v).max().unwrap_or(0);
        if max_source_sample_number - base_sample_number >= audio_renderer.flush_len * 3 {
            blog(LOG_ERROR, "[audio_renderer] some audio source not provide data\0".as_ptr().cast());
            for x in &mut audio_renderer.source_sample_number {
                if *x == base_sample_number {
                    *x = 0;
                }
            }
        }
        // 输出数据
        let min_source_sample_number = audio_renderer.source_sample_number.iter().filter_map(|v| {
            if *v >= base_sample_number {
                Some(*v)
            } else {
                None
            }
        }).min().unwrap_or(0);
        let sample_count = min_source_sample_number.saturating_sub(base_sample_number);
        if sample_count >= audio_renderer.flush_len {
            let half_index = audio_renderer.texture_buffer.len() / 2;
            // 一半是左声道，另一半是右声道
            fill_texture_buffer(&mut audio_renderer.texture_buffer[..half_index], audio_buffer[0].iter().take(sample_count).map(|v| *v), audio_renderer.width, audio_renderer.cell_width, audio_renderer.cell_height, audio_renderer.packet_index);
            fill_texture_buffer(&mut audio_renderer.texture_buffer[half_index..], audio_buffer[1].iter().take(sample_count).map(|v| *v), audio_renderer.width, audio_renderer.cell_width, audio_renderer.cell_height, audio_renderer.packet_index);
            truncate_front(&mut audio_buffer[0], sample_count);
            truncate_front(&mut audio_buffer[1], sample_count);
            audio_renderer.base_sample_number += sample_count;
            audio_renderer.packet_index += 1;
            modified = true;
        }
        // 缓冲长度过大时，清除 buffer，防止延迟过高
        if audio_buffer[0].len() >= audio_renderer.flush_len * 3 {
            blog(LOG_ERROR, "[audio_renderer] audio_buffer too long\0".as_ptr().cast());
            audio_buffer[0].clear();
            audio_buffer[1].clear();
            audio_renderer.source_sample_number.fill(0);
            audio_renderer.base_sample_number = 1;
        }
        // 此处释放 audio_buffer 的 Mutex
    }

    obs_enter_graphics();
    // 更新 texture 数据内容
    if modified {
        gs_texture_set_image(audio_renderer.texture, audio_renderer.texture_buffer.as_ptr(), (audio_renderer.width * 4) as _, false);
    }
    // 传进来的 effect 是 libobs/data/default.effect
    // libobs 调用 video_render 之前，已经指定了使用的是 technique Draw 的唯一一个 pass
    // 我们设置 texture2d image 参数即可
    gs_effect_set_texture(gs_effect_get_param_by_name(effect, "image\0".as_ptr().cast()), audio_renderer.texture);
    // gs_draw_sprite 时会自动构建矩形的 4 个顶点坐标 buffer、顶点 UV buffer、顶点索引 buffer
    // 会自动设置 vertex_shader 的 VertInOut vert_in 参数
    // gs_draw 时，device_draw 实现中会自动设置 float4x4 ViewProj 参数
    gs_draw_sprite(audio_renderer.texture, 0, 0, 0);
    obs_leave_graphics();
}
