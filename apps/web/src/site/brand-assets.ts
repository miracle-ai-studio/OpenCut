import symbol from "./assets/symbol.svg";
import symbolLight from "./assets/symbol-light.svg";
import logo from "./assets/logo.svg";
import logoLight from "./assets/logo-light.svg";
import text from "./assets/text.svg";
import textLight from "./assets/text-light.svg";
export const brandAssets = [
  {
    name: "Symbol",
    zh: "图标",
    variants: [
      { file: "symbol.svg", url: symbol, light: false },
      { file: "symbol-light.svg", url: symbolLight, light: true },
    ],
  },
  {
    name: "Logo",
    zh: "完整标志",
    variants: [
      { file: "logo.svg", url: logo, light: false },
      { file: "logo-light.svg", url: logoLight, light: true },
    ],
  },
  {
    name: "Wordmark",
    zh: "文字标志",
    variants: [
      { file: "text.svg", url: text, light: false },
      { file: "text-light.svg", url: textLight, light: true },
    ],
  },
];
function crc32(bytes: Uint8Array) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
export function zipAssets(
  files: { name: string; bytes: Uint8Array }[],
): Uint8Array<ArrayBuffer> {
  const local: Uint8Array[] = [],
    central: Uint8Array[] = [];
  let offset = 0;
  for (const file of files) {
    const name = new TextEncoder().encode(file.name);
    if (!/^[a-z-]+\.svg$/.test(file.name) || file.bytes.length > 1024 * 1024)
      throw new Error("invalid brand asset");
    const header = new Uint8Array(30 + name.length),
      h = new DataView(header.buffer),
      crc = crc32(file.bytes);
    h.setUint32(0, 0x04034b50, true);
    h.setUint16(4, 20, true);
    h.setUint32(14, crc, true);
    h.setUint32(18, file.bytes.length, true);
    h.setUint32(22, file.bytes.length, true);
    h.setUint16(26, name.length, true);
    header.set(name, 30);
    local.push(header, file.bytes);
    const directory = new Uint8Array(46 + name.length),
      d = new DataView(directory.buffer);
    d.setUint32(0, 0x02014b50, true);
    d.setUint16(4, 20, true);
    d.setUint16(6, 20, true);
    d.setUint32(16, crc, true);
    d.setUint32(20, file.bytes.length, true);
    d.setUint32(24, file.bytes.length, true);
    d.setUint16(28, name.length, true);
    d.setUint32(42, offset, true);
    directory.set(name, 46);
    central.push(directory);
    offset += header.length + file.bytes.length;
  }
  const length = central.reduce((total, part) => total + part.length, 0),
    end = new Uint8Array(22),
    e = new DataView(end.buffer);
  e.setUint32(0, 0x06054b50, true);
  e.setUint16(8, files.length, true);
  e.setUint16(10, files.length, true);
  e.setUint32(12, length, true);
  e.setUint32(16, offset, true);
  const result = new Uint8Array(offset + length + 22);
  let position = 0;
  for (const part of [...local, ...central, end]) {
    result.set(part, position);
    position += part.length;
  }
  return result;
}
export async function downloadBrand() {
  const files = [];
  for (const asset of brandAssets.flatMap((item) => item.variants)) {
    const response = await fetch(asset.url);
    if (!response.ok) throw new Error("brand download failed");
    files.push({
      name: asset.file,
      bytes: new Uint8Array(await response.arrayBuffer()),
    });
  }
  const url = URL.createObjectURL(
    new Blob([zipAssets(files)], { type: "application/zip" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = "opencut-brand-assets.zip";
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}
