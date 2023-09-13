# OBS 音频渲染器

将音频的采样信息渲染成视频画面，每个声道的每个采样对应一个像素点的灰度值。

通过抖动可以让渲染结果的音频采样深度大概是 10bit，虽然会有明显的底噪，但听感上可以接受。

左右声道是分开渲染的，因为 Web Audio API 的 AudioBuffer.getChannelData https://developer.mozilla.org/en-US/docs/Web/API/AudioBuffer/getChannelData 是分声道写入数据的。

## 推流

首先，在声音源上添加一个 Audio Capture 滤镜。注意，可以修改这个滤镜名称。

然后，在添加一个 Audio Renderer 视频源，并选择之前的 Audio Capture 滤镜作为数据源。

设置 Audio Renderer 编码区域的宽度、高度，以及每个采样数据的编码的小方格宽度、高度。还有最小缓冲长度。

最小缓冲长度是为了减轻浏览器端解码的负担。比如，设置为 2400，因为声音采样率是 48000Hz，所以最小缓冲 0.05s，客户端每 3 帧解码一次即可实现声音连续播放。

最小缓冲长度不能超过编码区域可容纳的最大数据量。如果超出之后，会有错误日志。查看错误日志的方法：菜单栏 -> 帮助 -> 日志文件 -> 查看当前日志。

通常，默认参数即可。

## 观看

安装 Tampermonkey 浏览器插件 https://www.tampermonkey.net/ ，Chrome 浏览器和 Edge 浏览器都可以直接在商店进行安装。

然后访问 https://github.com/ganlvtech/obs-audio-renderer/raw/main/userscript/audio_decode.user.js 安装视频解码脚本。

然后访问一个声音经过编码的直播间，在右上角的插件中找到“obs-audio-renderer 解码”

* 使用默认参数解码视频：这个就是使用默认参数 `0,0,32,1072,2,2` 解码。
* 自定义参数解码视频：这个可以指定自定义密码，自定义区域，示例值如下：
  * `0,0,32,1072,2,2`
  * `0,0,1920,16,2,2`
  * `0,0,128,1072,4,4`

然后，将直播声音静音，仅收听通过画面解码的声音。

## LICENSE

OBS 插件部分使用 GPL-2.0 License

Web Audio 解码部分使用 MIT License
