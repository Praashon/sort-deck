import { Buffer } from 'node:buffer';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

function crc32(buf) {
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) {
    crc ^= buf[i];
    for (let j = 0; j < 8; j += 1) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

function encodePng(width, height, rgba) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const stride = width * 4 + 1;
  const raw = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * stride] = 0; // filter None
    rgba.copy(raw, y * stride + 1, y * width * 4, (y + 1) * width * 4);
  }

  const compressed = zlib.deflateSync(raw, { level: 9 });
  return Buffer.concat([
    signature,
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', compressed),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function insideRoundedRect(x, y, rx, ry, rw, rh, radius) {
  if (x < rx || x > rx + rw || y < ry || y > ry + rh) {
    return false;
  }
  const cx = Math.min(Math.max(x, rx + radius), rx + rw - radius);
  const cy = Math.min(Math.max(y, ry + radius), ry + rh - radius);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= radius * radius;
}

// Samples the 32x32 architectural emblem geometry at normalized coordinates (0..32).
function sampleEmblem(nx, ny) {
  const inOuter = insideRoundedRect(nx, ny, 1.0, 1.0, 30.0, 30.0, 7.0);
  if (!inOuter) {
    return [0, 0, 0, 0];
  }
  const inInner = insideRoundedRect(nx, ny, 2.0, 2.0, 28.0, 28.0, 6.1);
  if (!inInner) {
    return [46, 46, 54, 255]; // #2e2e36 crisp border
  }

  // Top alabaster card with clean folder tab step on the top edge
  const inLowerCard = insideRoundedRect(nx, ny, 7.0, 18.5, 18.0, 6.5, 1.5);
  const inFolderTab = insideRoundedRect(nx, ny, 7.0, 16.5, 9.2, 4.0, 1.4);
  if (inLowerCard || inFolderTab) {
    return [244, 244, 245, 255]; // #f4f4f5
  }

  // Mineral teal index bar locking the right column
  if (insideRoundedRect(nx, ny, 21.0, 7.0, 4.0, 9.2, 1.4)) {
    return [44, 181, 160, 255]; // #2cb5a0
  }

  // Middle card
  if (insideRoundedRect(nx, ny, 7.0, 12.5, 12.6, 3.8, 1.3)) {
    return [124, 124, 139, 255]; // #7c7c8b
  }

  // Top steel card
  if (insideRoundedRect(nx, ny, 7.0, 7.0, 12.6, 3.8, 1.3)) {
    return [70, 70, 82, 255]; // #464652
  }

  return [20, 20, 23, 255]; // #141417 obsidian base
}

function renderEmblemRgba(size) {
  const rgba = Buffer.alloc(size * size * 4);
  const ss = 4; // 4x4 supersampling for clean anti-aliased edges
  const invTotal = 1 / (ss * ss);

  for (let py = 0; py < size; py += 1) {
    for (let px = 0; px < size; px += 1) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;

      for (let sy = 0; sy < ss; sy += 1) {
        for (let sx = 0; sx < ss; sx += 1) {
          const u = ((px + (sx + 0.5) / ss) / size) * 32;
          const v = ((py + (sy + 0.5) / ss) / size) * 32;
          const [sr, sg, sb, sa] = sampleEmblem(u, v);
          r += sr;
          g += sg;
          b += sb;
          a += sa;
        }
      }

      const idx = (py * size + px) * 4;
      rgba[idx] = Math.round(r * invTotal);
      rgba[idx + 1] = Math.round(g * invTotal);
      rgba[idx + 2] = Math.round(b * invTotal);
      rgba[idx + 3] = Math.round(a * invTotal);
    }
  }
  return rgba;
}

function buildIco(frames) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // ICO type
  header.writeUInt16LE(frames.length, 4);

  const dirEntries = [];
  let offset = 6 + frames.length * 16;

  for (const frame of frames) {
    const entry = Buffer.alloc(16);
    entry[0] = frame.size >= 256 ? 0 : frame.size;
    entry[1] = frame.size >= 256 ? 0 : frame.size;
    entry[2] = 0;
    entry[3] = 0;
    entry.writeUInt16LE(1, 4); // planes
    entry.writeUInt16LE(32, 6); // bpp
    entry.writeUInt32LE(frame.png.length, 8);
    entry.writeUInt32LE(offset, 12);
    offset += frame.png.length;
    dirEntries.push(entry);
  }

  return Buffer.concat([header, ...dirEntries, ...frames.map((f) => f.png)]);
}

const iconsDir = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'src-tauri',
  'icons',
);
fs.mkdirSync(iconsDir, { recursive: true });

const png32 = encodePng(32, 32, renderEmblemRgba(32));
const png128 = encodePng(128, 128, renderEmblemRgba(128));
const png256 = encodePng(256, 256, renderEmblemRgba(256));
const png16 = encodePng(16, 16, renderEmblemRgba(16));
const png48 = encodePng(48, 48, renderEmblemRgba(48));

fs.writeFileSync(path.join(iconsDir, '32x32.png'), png32);
fs.writeFileSync(path.join(iconsDir, '128x128.png'), png128);
fs.writeFileSync(path.join(iconsDir, '128x128@2x.png'), png256);
fs.writeFileSync(
  path.join(iconsDir, 'icon.ico'),
  buildIco([
    { size: 16, png: png16 },
    { size: 32, png: png32 },
    { size: 48, png: png48 },
    { size: 256, png: png256 },
  ]),
);

console.log('Generated SortDeck logo icons in src-tauri/icons/.');
