// ==UserScript==
// @name         obs-audio-renderer 音频解码
// @namespace    http://tampermonkey.net/
// @version      0.1
// @description  try to take over the world!
// @author       Ganlv
// @homepage     https://github.com/ganlvtech/obs-audio-renderer
// @match        https://live.bilibili.com/*
// @icon         https://live.bilibili.com/favicon.ico
// @grant        GM_registerMenuCommand
// @grant        GM_unregisterMenuCommand
// @grant        GM_getValue
// @grant        GM_setValue
// ==/UserScript==

(function () {
  'use strict';

  // The MIT License (MIT)
  //
  // Copyright (c) 2023 Ganlv
  //
  // Permission is hereby granted, free of charge, to any person obtaining a copy
  // of this software and associated documentation files (the "Software"), to deal
  // in the Software without restriction, including without limitation the rights
  // to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  // copies of the Software, and to permit persons to whom the Software is
  // furnished to do so, subject to the following conditions:
  //
  // The above copyright notice and this permission notice shall be included in
  // all copies or substantial portions of the Software.
  //
  // THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  // IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  // FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  // AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  // LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  // OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
  // THE SOFTWARE.

  // WebGL 比 Canvas2D 的性能高很多，大概是 3ms 和 30ms 的差别，Canvas2D 的实现经常会出现因为解码速度跟不上导致声音卡顿的问题。

  function newGetVideoRgbaDataWebGL(video, videoWidth, videoHeight) {
    const canvas = document.createElement('canvas');
    canvas.width = videoWidth;
    canvas.height = videoHeight;
    const gl = canvas.getContext('webgl');
    if (!gl) {
      throw new Error('not support webgl');
    }

    // 编译 shader
    const vertex_shader = gl.createShader(gl.VERTEX_SHADER);
    const fragment_shader = gl.createShader(gl.FRAGMENT_SHADER);
    gl.shaderSource(vertex_shader, `
attribute vec4 aVertexPosition;
attribute vec2 aTextureCoord;
varying highp vec2 vTextureCoord;
void main() {
  gl_Position = aVertexPosition;
  vTextureCoord = aTextureCoord;
}`);
    gl.shaderSource(fragment_shader, `
varying highp vec2 vTextureCoord;
uniform sampler2D uSamplerVideo;
void main(void) {
  gl_FragColor = texture2D(uSamplerVideo, vTextureCoord);
}`);
    gl.compileShader(vertex_shader);
    gl.compileShader(fragment_shader);
    const shader_program = gl.createProgram();
    gl.attachShader(shader_program, vertex_shader);
    gl.attachShader(shader_program, fragment_shader);
    gl.linkProgram(shader_program);
    if (!gl.getProgramParameter(shader_program, gl.LINK_STATUS)) {
      throw new Error(`Unable to initialize the shader program: ${gl.getProgramInfoLog(shader_program)}`);
    }
    gl.useProgram(shader_program);

    // 清空场景
    gl.clearColor(0.0, 0.0, 0.0, 0.0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    // 准备顶点坐标数据
    const position_buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, position_buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([1, 1, -1, 1, 1, -1, -1, -1]), gl.STATIC_DRAW); // 右上 左上 右下 左下 // OpenGL 的坐标是右手系，左下角是 -1 -1，右上角是 1 1
    gl.vertexAttribPointer(gl.getAttribLocation(shader_program, "aVertexPosition"), 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(gl.getAttribLocation(shader_program, "aVertexPosition"));

    // 准备顶点 UV 数据
    const texture_coord_buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, texture_coord_buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([1, 1, 0, 1, 1, 0, 0, 0]), gl.STATIC_DRAW); // 右下 左下 右上 左上 // OpenGL 的贴图左上角是 0 0，右下角是 1 1。因为 readPixels 坐标是上下颠倒的的原因，这里的顶点 UV 需要上下颠倒过来
    gl.vertexAttribPointer(gl.getAttribLocation(shader_program, "aTextureCoord"), 2, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(gl.getAttribLocation(shader_program, "aTextureCoord"));

    // 准备视频贴图
    gl.activeTexture(gl.TEXTURE0);
    const video_texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, video_texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST); // 尺寸非 2 的幂的贴图，只能使用 NEAREST
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST); // 尺寸非 2 的幂的贴图，只能使用 NEAREST
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); // 尺寸非 2 的幂的贴图，只能使用 CLAMP_TO_EDGE
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE); // 尺寸非 2 的幂的贴图，只能使用 CLAMP_TO_EDGE
    gl.uniform1i(gl.getUniformLocation(shader_program, "uSamplerVideo"), 0);

    return (x, y, width, height) => {
      const uint8Array = new Uint8Array(width * height * 4);
      gl.activeTexture(gl.TEXTURE0);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, video);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      gl.readPixels(x, y, width, height, gl.RGBA, gl.UNSIGNED_BYTE, uint8Array); // readPixels 使用的 x y 是从 framebuffer 左下角开始计算的，framebuffer 左下角是 0 0，右上角是 canvas.width canvas.height，并且读到内存的 RGBA 数据是从下往上一行一行依次排列的。
      return uint8Array;
    };
  }

  function newGetVideoRgbaDataCanvas2D(video, videoWidth, videoHeight) {
    const canvas = document.createElement('canvas');
    canvas.width = videoWidth;
    canvas.height = videoHeight;
    const ctx = canvas.getContext('2d', {
      willReadFrequently: true,
      alpha: false,
    });
    ctx.imageSmoothingEnabled = false;

    return (x, y, width, height) => {
      ctx.drawImage(video, 0, 0); // canvas 2d 的 drawImage 真的太慢了，经常出现需要 33ms 才能画完，相当于卡了两帧
      const imageData = ctx.getImageData(x, y, width, height);
      return imageData.data; // Uint8ClampedArray
    };
  }

  /**
   * 从 RGB 颜色解码为音频采样
   *
   * @param {number} r 16 ~ 255
   * @param {number} g 16 ~ 255
   * @param {number} b 16 ~ 255
   * @returns {number} -1.0 ~ 1.0
   */
  function decodeAudioSample(r, g, b) {
    return ((g - 136.0) * 2 + (r - 136.0) + (b - 136.0)) / 4.0 / 120.0;
  }

  /**
   * @param {Uint8Array|Uint8ClampedArray} data RGBA 数据 0 ~ 255。长度至少应该为 width * height * 4
   * @param {number} width
   * @param {number} height
   * @param {number} cellWidth
   * @param {number} cellHeight
   * @returns {Float32Array} 返回声音数据，范围是 -1.0 ~ 1.0
   */
  function decodeRgbaDataToAudio(data, width, height, cellWidth, cellHeight) {
    if (width * height * 4 > data.length) {
      throw new Error('data RGBA 数据的长度至少应该为 width * height * 4');
    }
    if (width % cellWidth !== 0) {
      throw new Error('width 必须能整除 cellWidth');
    }
    if (height % cellHeight !== 0) {
      throw new Error('height 必须能整除 cellHeight');
    }
    const cellPixelCount = cellWidth * cellHeight;
    const audioBuffer = new Float32Array(width * height);
    let audioBufferIndex = 0;

    for (let y = 0; y < height; y += cellHeight) {
      for (let x = 0; x < width; x += cellWidth) {
        let r = 0;
        let g = 0;
        let b = 0;
        for (let j = 0; j < cellHeight; j++) {
          for (let i = 0; i < cellWidth; i++) {
            const index = 4 * ((y + j) * width + (x + i));
            r += data[index];
            g += data[index + 1];
            b += data[index + 2];
          }
        }
        r = r / cellPixelCount;
        g = g / cellPixelCount;
        b = b / cellPixelCount;
        if (r < 12 && g < 12 && b < 12) {
          return audioBuffer.subarray(0, audioBufferIndex);
        }
        audioBuffer[audioBufferIndex] = decodeAudioSample(r, g, b);
        audioBufferIndex++;
      }
    }
    return audioBuffer;
  }

  /**
   * 创建音频播放器
   *
   * @returns {(function(Float32Array[]): void)} 每次调用都会播放声音，如果间隔小于 0.5 秒则连续播放，大于 0.5 秒则重新播放
   */
  function newAudioPlayer() {
    // 注意：
    // 需要在 edge://settings/content/mediaAutoplay (设置 -> Cookie 和网站权限 -> 站点权限 -> 媒体自动播放) 中添加 live.bilibili.com
    // 这样才能不通过用户操作，直接通过代码自动开始播放声音

    const audioCtx = new AudioContext({
      sampleRate: 48000,
    });

    let startTime = 0;
    let chasing = false;

    /**
     * @param {Float32Array[]} channelsData 左右声道的数据
     */
    return (channelsData) => {
      const audioBuffer = new AudioBuffer({
        length: channelsData[0].length,
        numberOfChannels: channelsData.length,
        sampleRate: audioCtx.sampleRate,
      });
      channelsData.forEach((channelData, i) => {
        audioBuffer.copyToChannel(channelData, i);
      });

      // 说明：audioCtx.currentTime 是随时间线性增长的
      // startTime 是上一段 AudioBuffer 播放的结束时间，用于确保声音连续的
      // source.start(startTime) 的 startTime 参数必须大于等于 audioCtx.currentTime
      // 指定未来的某个时间开始播放这段 AudioBuffer
      // 所以首次播放声音时，会让 startTime = audioCtx.currentTime + 0.2 预留一小段缓冲时间
      // 同时，在运行中可能存在因为解码卡顿丢掉一段或几段 AudioBuffer，导致这时 startTime < audioCtx.currentTime
      // 如果进度差距小于 0.5 秒，我们采用重复播放的当前 audioBuffer 的方式弥补丢掉的部分，追上进度（此方法可以听出卡顿，但是卡顿效果可以接受）
      // 如果进度差距过大，那么从新的时间点开始播放，从 startTime = audioCtx.currentTime + 0.2 开始播放
      const currentTime = audioCtx.currentTime;
      const timeDiff = currentTime - startTime;
      if (timeDiff > 0.5 || timeDiff < -0.5) {
        chasing = false;
        startTime = audioCtx.currentTime + 0.2;
      } else if (timeDiff >= 0) {
        chasing = true;
        console.log('chase start', timeDiff);
      } else {
        if (chasing) {
          if (timeDiff < -0.2) {
            chasing = false;
            console.log('chase finish', timeDiff);
          }
        }
      }

      if (chasing) {
        console.log('chasing', timeDiff);
        // 额外播放 1 次，用于追上进度
        const source2 = new AudioBufferSourceNode(audioCtx, {
          buffer: audioBuffer,
        });
        source2.connect(audioCtx.destination);
        source2.start(startTime);
        startTime += audioBuffer.duration;
      }
      const source = new AudioBufferSourceNode(audioCtx, {
        buffer: audioBuffer,
      });
      source.connect(audioCtx.destination);
      source.start(startTime);
      startTime += audioBuffer.duration;
    }
  }

  /**
   * 判断数组是否相似，数组长度相差 1%，数据内容相差 1%
   *
   * @param {Float32Array} a -1.0 ~ 1.0
   * @param {Float32Array} b -1.0 ~ 1.0
   */
  function isFloat32ArraySimilar(a, b) {
    const len = Math.min(a.length, b.length);
    if (Math.abs(a.length - b.length) / len < 0.01) {
      let sumDiff = 0;
      for (let i = 0; i < len; i++) {
        sumDiff += Math.abs(a[i] - b[i]);
      }
      if (sumDiff / len < 0.01) {
        return true;
      }
    }
    return false;
  }

  function run(x, y, width, height, cellWidth, cellHeight) {
    if (width <= 0) {
      throw new Error('width 必须 >= 0');
    }
    if (height <= 0) {
      throw new Error('height 必须 >= 0');
    }
    if (cellWidth <= 0) {
      throw new Error('cellWidth 必须 >= 0');
    }
    if (cellHeight <= 0) {
      throw new Error('cellHeight 必须 >= 0');
    }
    if (height % 2 !== 0) {
      throw new Error('height 必须是 2 的倍数');
    }
    if (width % cellWidth !== 0) {
      throw new Error('width 必须能整除 cellWidth');
    }
    if ((height / 2) % cellHeight !== 0) {
      throw new Error('height 的一半必须能整除 cellHeight');
    }

    const video = document.querySelector('video');
    let getVideoRgbaData;
    try {
      getVideoRgbaData = newGetVideoRgbaDataWebGL(video, video.videoWidth, video.videoHeight);
    } catch (e) {
      if (e.message === 'not support webgl') {
        getVideoRgbaData = newGetVideoRgbaDataCanvas2D(video, video.videoWidth, video.videoHeight);
      } else {
        throw e;
      }
    }
    const playAudioBuffer = newAudioPlayer();
    let prevLeftChannelData = new Float32Array(0);
    const update = () => {
      if (video.paused) {
        requestAnimationFrame(update);
        return;
      }

      const rgbaData = getVideoRgbaData(x, y, width, height);
      const leftChannelData = decodeRgbaDataToAudio(rgbaData.subarray(0, rgbaData.length / 2), width, height / 2, cellWidth, cellHeight);
      if (leftChannelData.length >= 240) { // buffer 太短不播放
        if (!isFloat32ArraySimilar(leftChannelData, prevLeftChannelData)) { // audio buffer 和上一帧相似则不播放
          const rightChannelData = decodeRgbaDataToAudio(rgbaData.subarray(rgbaData.length / 2, rgbaData.length), width, height / 2, cellWidth, cellHeight);
          playAudioBuffer([leftChannelData, rightChannelData]);
          prevLeftChannelData = leftChannelData;
        }
      }
      setTimeout(update, 16); // 后台需要播放音乐，所以必须使用 setTimeout。需要在 设置 -> 系统和性能
    }
    update();
  }

  GM_registerMenuCommand("使用默认参数解码音频", () => {
    run(0, 0, 32, 1072, 2, 2);
  });
  GM_registerMenuCommand("自定义参数解码音频", () => {
    const config = window.prompt("x,y,width,height,cell_width,cell_height", GM_getValue("config", "0,0,32,1072,2,2"));
    if (config) {
      GM_setValue("config", config);
      const [x, y, width, height, cellWidth, cellHeight] = config.split(',').map((s) => s.trim()).map((s) => parseInt(s));
      run(x, y, width, height, cellWidth, cellHeight);
    }
  });
})();
