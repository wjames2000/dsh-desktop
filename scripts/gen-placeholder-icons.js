#!/usr/bin/env node
/**
 * 生成 Tauri 占位图标（纯色 RGBA PNG）。
 *
 * 用 Node 内置 zlib 手写 PNG 编码，不依赖任何第三方包。
 * 后续正式图标可覆盖 src-tauri/icons/ 下的产物，或直接替换本脚本。
 *
 * PNG 结构：8 字节签名 + IHDR chunk + IDAT chunk（zlib 压缩的扫描线，
 * 每行以 filter byte 0 开头）+ IEND chunk。CRC32 用查表法手写。
 * 注意：Tauri 要求图标为 RGBA（color type 6，带 alpha 通道）。
 */
'use strict';

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

// ---------- CRC32（查表法） ----------
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

// ---------- PNG chunk ----------
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

/**
 * 生成纯色 PNG。
 * @param {number} width
 * @param {number} height
 * @param {[number, number, number]} rgb  [r, g, b]，0-255（alpha 恒为 255）
 * @returns {Buffer}
 */
function encodeSolidPng(width, height, [r, g, b]) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  // IHDR：宽、高、位深 8、颜色类型 6（RGBA）、压缩/滤波/隔行 0
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  // 扫描线：每行 = 1 字节 filter(0) + width*4 字节 RGBA
  const row = Buffer.alloc(1 + width * 4);
  for (let x = 0; x < width; x++) {
    row[1 + x * 4] = r;
    row[2 + x * 4] = g;
    row[3 + x * 4] = b;
    row[4 + x * 4] = 255; // alpha
  }
  const raw = Buffer.concat(Array.from({ length: height }, () => row));

  return Buffer.concat([
    sig,
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------- 主流程 ----------
const ICONS_DIR = path.join(__dirname, '..', 'src-tauri', 'icons');
// 深蓝色占位
const COLOR = [13, 94, 217];

const sizes = [
  ['32x32.png', 32, 32],
  ['128x128.png', 128, 128],
  ['128x128@2x.png', 256, 256],
  ['1024.png', 1024, 1024], // 母图，供后续生成正式图标集使用
];

fs.mkdirSync(ICONS_DIR, { recursive: true });
for (const [name, w, h] of sizes) {
  const file = path.join(ICONS_DIR, name);
  fs.writeFileSync(file, encodeSolidPng(w, h, COLOR));
  console.log(`generated ${path.relative(process.cwd(), file)} (${w}x${h})`);
}
