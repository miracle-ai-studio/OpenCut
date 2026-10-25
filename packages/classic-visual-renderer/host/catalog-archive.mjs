// Private native archive adapter. No network, filesystem paths, eval or browser.
// The reviewed Kenney archive is the only input; Node's bounded inflater supplies
// DEFLATE. This module also exports pure parsing for offline negative tests.
import { createHash } from "node:crypto";
import { inflateRawSync } from "node:zlib";
import { pathToFileURL } from "node:url";
const HASH = "946fc23a63d535d693eb31b2eabb80c8c28d6351e2186b344ceb71b2cb1d5eb6";
const LICENSE = "4f88ab3c885c87874834441a0d009cea8942f57461d7b870be65cf4e31362073";
const SOUNDS = new Map([["click1",{"size":4983,"sha256":"59175ac17cd49a68dd736285738441287636112a84a6f7ce0d89921bda5a5360","crc32":853922102}],["click2",{"size":4656,"sha256":"c394b46bc6737f7f7edfbfa3031ce621b7a43d498d456d6198364eee9cd3e306","crc32":998991823}],["click3",{"size":4880,"sha256":"e3e7fc7ccd9c5cfdf77bffa05aee10d4973812076c76dc889ba2b16ea434127f","crc32":3476567237}],["click4",{"size":4558,"sha256":"9c02bbfc872b50eb8750adf4b7c343120f19f5d2f04ab06a7a8948368a77b872","crc32":2769900476}],["click5",{"size":4532,"sha256":"52d41e0bb012731bf2a391d53ffb52265ef21589c7701e66b391d5e36a65b293","crc32":1679522797}],["mouseclick1",{"size":4686,"sha256":"ece1210c62ccae6c9ea4f4c59466d9d453d6b357c7d4b1a6c31196e5e6fea1bf","crc32":2807776395}],["mouserelease1",{"size":4775,"sha256":"23cb78cea76d8f1bb1266af228101a09b5d62063f4ac87ead15b1d395c6181f0","crc32":3468297001}],["rollover1",{"size":5932,"sha256":"4eb47e2e1540c44847b47a89825e21335c820a4e54cacf8c4c1eb5100e36af42","crc32":84337839}],["rollover2",{"size":4701,"sha256":"d0bc0fcba496d73df1da8c145e5330df75f87ab5a4f33a9b71461b006b5c338e","crc32":4178801973}],["rollover3",{"size":4778,"sha256":"641df2c95468f1f035ff0ae964129da10543b4ea8ff6c13f66bcbd899dcfc113","crc32":3818126802}],["rollover4",{"size":5188,"sha256":"8b70e47aa1d51d220a3609a02cc50ceed6e05a20f51c6f8daaf5fd83eae42cf3","crc32":2266368305}],["rollover5",{"size":5258,"sha256":"3e1805b914a66e63695913d19ba98c5ec584cc69d29c7167d83c460ab752a5d3","crc32":738430378}],["rollover6",{"size":5385,"sha256":"18f6f69f5fdbf55c175ca4eca8447d4ad5d8a315efd985038424f922dadefad8","crc32":555573596}],["switch1",{"size":6104,"sha256":"efdd1d1e2904fb2d81259cd96bb80101caceae94be02baa4b5310714a6708b19","crc32":1789266336}],["switch2",{"size":6042,"sha256":"aa8ad6e4745e87c84c24a0335e0e0f8629ccbbc474ccfd544fe07d7f3a1b2280","crc32":690598311}],["switch3",{"size":6270,"sha256":"75d87dc60b7d29df838530af8c9e28205102234e07b5f1e849aa3d95c3922bd0","crc32":1239060896}],["switch4",{"size":6477,"sha256":"687a42c24e5d25be1a256f35a439fe858548fe76d43ccfb17db546618b0d36db","crc32":3319238945}],["switch5",{"size":6181,"sha256":"8a5c85c0009cfd985634e8fdb4e0350690be5d85fd49f43a90b35c7de0bef7c0","crc32":418214590}],["switch6",{"size":6610,"sha256":"34d37cbfe6785cab2f3217b613fc976ba05b869c0ce61381eafa67ee257ecc9d","crc32":1267406260}],["switch7",{"size":5707,"sha256":"7a3032d1dae63d1096cf2f1e59cb43e598d5229dcb9617e9d3fb13e96c78d5f2","crc32":533488591}],["switch8",{"size":6253,"sha256":"c7bf07f8d58ac62faac2427868619631b1a89b0ab08e87c7c01621a086fa1873","crc32":4185782451}],["switch9",{"size":6009,"sha256":"069861740681c528866483e3d528d79034f24d744c4fc79f7045b7d9ed999e26","crc32":3563995372}],["switch10",{"size":6456,"sha256":"bf61b3f3af39b2c680e70ee2f791786edd2bca2016ea683947059f079849bbf3","crc32":2650753429}],["switch11",{"size":6248,"sha256":"a8d445bd96fe3842ff2dcdd42ae728330baa8928032ca5f3d5cd4b926929550a","crc32":4236795568}],["switch12",{"size":4588,"sha256":"dab0c21eacb6536724d139c27c0d60c932c87171752dc8d24d7a826694377f94","crc32":3079840206}],["switch13",{"size":4467,"sha256":"54d1e63b907274bb415a8de7f652194756a8eeab4079a7cf769ec153b93a436a","crc32":220185973}],["switch14",{"size":4522,"sha256":"c640b370738c816eda3d1196d8a8dda7130825b4094f3bb3cbba22a9f6f9b7c9","crc32":1864008963}],["switch15",{"size":6325,"sha256":"5a20a9d5365dec8fec1311731540f56429cab38822326133c7ce60fbcf9fd275","crc32":1896682286}],["switch16",{"size":6991,"sha256":"d2e9dd4fd2918132f8400e270701384e93139f2207ac115705a2c183cf3d2098","crc32":1515398932}],["switch17",{"size":6686,"sha256":"d09974d4cf5ccb2d75647e2464cd424e7e3c7a0374e73e8a22577662031a3677","crc32":1929180676}],["switch18",{"size":7633,"sha256":"ba759707fc2e55f4e7d1edd1dd78a4110267f4dc564edfb989c00f062ef69d8c","crc32":2139328658}],["switch19",{"size":7177,"sha256":"63753f884e4e0d44b0939bee9f048b32e658d996958930363c0579ffa0f810c1","crc32":65997755}],["switch20",{"size":6965,"sha256":"276b402ee4840f88a8df4eca6ece029b61c5840fa9149b0c5008fe4bda89cc21","crc32":4065580871}],["switch21",{"size":7453,"sha256":"8a3fdc5d35c74ae4ce4ec75c1f85c7f5fac1a2a6ac838353b09c48c5d7888fc9","crc32":2363826167}],["switch22",{"size":7116,"sha256":"913261ad55fa0692902e4027a7a139844e7589d30c4286ab10e6d637f28b1d9f","crc32":2839056118}],["switch23",{"size":7177,"sha256":"148d91722dd819b450d8b50e65686d9b1d39d38ca05b361caa387bcbd2a90d81","crc32":2790349244}],["switch24",{"size":6340,"sha256":"aec5e91eef35c4a64d3bec7a25e54a68fa0448f4cf3179455e18c72c3dda60e9","crc32":769721988}],["switch25",{"size":7130,"sha256":"ba31fc9a780a6908809154e0108e8d2728b5b9a6e769c8f56d61d920fba100ae","crc32":2338858493}],["switch26",{"size":6031,"sha256":"1a628e2fbede99afdb57533fa367f352fede596df18c91deb841a21f8171654a","crc32":3250693919}],["switch27",{"size":6445,"sha256":"a6a3a5fa54037e3b69c0fe7f2d9b50422cb23dd616bb7f72ac0a2a028a810028","crc32":3632737352}],["switch28",{"size":5916,"sha256":"2f7b627d85ce1530faef5374d3826bfca1ff2823b1b41d5f908448976061e2d4","crc32":2290202117}],["switch29",{"size":6769,"sha256":"b2923ed170e963b5e211bf73e75f8eeb41f6b49a46eb52f3b3fc43234c713a16","crc32":3444006271}],["switch30",{"size":7151,"sha256":"d220abb5218fccc6d37649e17e7cce2a4e253f474332d9ea2b0f0518264aff5a","crc32":3472144926}],["switch31",{"size":8180,"sha256":"bca5f3047e73d28f6619d49a45dee1984340c165da5fa49b239068c2cb9b4be1","crc32":2925166139}],["switch32",{"size":7868,"sha256":"b740f507ad386a1d09bb5ca69a75231656a71375df11348fcd2d5e323b8d6727","crc32":3827247959}],["switch33",{"size":8530,"sha256":"977ecead4b27cf9801aed433bfa4bdfcf49444c15990d27d2e472401a1fb2742","crc32":798507948}],["switch34",{"size":7910,"sha256":"6e7943ed133c1b559d009da5028da0a318fee0b1bcfe6e4fa49744ba2cd3975b","crc32":865238425}],["switch35",{"size":6935,"sha256":"1ec7f5a76318f88c499d37567167f565cb564266d52b37112aac6467fe4126cb","crc32":298821472}],["switch36",{"size":6616,"sha256":"1dd3cf29291219ab2eed0292d5c3b06b1e102ea70821099e741fc127734ffa04","crc32":2806783924}],["switch37",{"size":6822,"sha256":"25faa57a3e819a3e3a2191934226e540467fcd11680131a7a77b47271048327e","crc32":2740210048}],["switch38",{"size":7303,"sha256":"d5c164ba622e3c7904eb0f442a81cff9985bebbd8951202348d2a7c8a449309b","crc32":1874904917}]]);
const MAX_ENTRY = 262144, MAX_TOTAL = 8 * 1024 * 1024;
const fail = () => { throw Error("CATALOG_ARCHIVE_INVALID"); };
const sha = b => createHash("sha256").update(b).digest("hex");
export function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) { crc ^= byte; for (let i = 0; i < 8; ++i) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
  return (crc ^ 0xffffffff) >>> 0;
}
export function extractZipEntry(archive, selected) {
  if (!Buffer.isBuffer(archive) || archive.length < 22 || archive.length > 500000) fail();
  let end = archive.length - 22;
  while (end >= Math.max(0, archive.length - 65557) && archive.readUInt32LE(end) !== 0x06054b50) --end;
  if (end < 0 || archive.readUInt32LE(end) !== 0x06054b50) fail();
  const count = archive.readUInt16LE(end + 10), size = archive.readUInt32LE(end + 12), start = archive.readUInt32LE(end + 16);
  if (archive.readUInt16LE(end + 4) || archive.readUInt16LE(end + 6) || archive.readUInt16LE(end + 8) !== count || !count || count > 256 || count === 65535 || start + size !== end || end + 22 + archive.readUInt16LE(end + 20) !== archive.length) fail();
  let at = start, total = 0, result = null;
  const names = new Set(), spans = [];
  for (let i = 0; i < count; i++) {
    if (at + 46 > end || archive.readUInt32LE(at) !== 0x02014b50) fail();
    const flags = archive.readUInt16LE(at + 8), method = archive.readUInt16LE(at + 10), crc = archive.readUInt32LE(at + 16), compressed = archive.readUInt32LE(at + 20), size = archive.readUInt32LE(at + 24);
    const n = archive.readUInt16LE(at + 28), extra = archive.readUInt16LE(at + 30), comment = archive.readUInt16LE(at + 32), offset = archive.readUInt32LE(at + 42);
    const mode = archive.readUInt32LE(at + 38) >>> 16;
    if (!n || at + 46 + n + extra + comment > end || flags & ~0x80e || ![0, 8].includes(method) || size > MAX_ENTRY || compressed > 500000 || archive.readUInt16LE(at + 34) || (mode & 0xf000) === 0xa000 || ![0, 0x4000, 0x8000].includes(mode & 0xf000)) fail();
    const nameBytes = archive.subarray(at + 46, at + 46 + n), name = nameBytes.toString("utf8");
    if (!Buffer.from(name).equals(nameBytes) || /[\\\x00-\x1f\x7f:]/.test(name) || name.startsWith("/") || name.split("/").some((p, j, all) => p === "." || p === ".." || (!p && j !== all.length - 1)) || names.has(name)) fail();
    names.add(name); total += size; if (total > MAX_TOTAL) fail();
    // Reject Zip64 and malformed extra fields in both directories, irrespective
    // of whether the selected entry uses them.
    const checkExtra = (from, length) => { for (let p = from; p < from + length;) { if (p + 4 > from + length) fail(); const id = archive.readUInt16LE(p), len = archive.readUInt16LE(p + 2); if (id === 1 || p + 4 + len > from + length) fail(); p += 4 + len; } };
    checkExtra(at + 46 + n, extra);
    if (offset + 30 > start || archive.readUInt32LE(offset) !== 0x04034b50 || archive.readUInt16LE(offset + 6) !== flags || archive.readUInt16LE(offset + 8) !== method) fail();
    const ln = archive.readUInt16LE(offset + 26), le = archive.readUInt16LE(offset + 28), body = offset + 30 + ln + le;
    if (body + compressed > start || ln !== n || !archive.subarray(offset + 30, offset + 30 + ln).equals(nameBytes)) fail();
    checkExtra(offset + 30 + ln, le);
    if (!(flags & 8) && (archive.readUInt32LE(offset + 14) !== crc || archive.readUInt32LE(offset + 18) !== compressed || archive.readUInt32LE(offset + 22) !== size)) fail();
    let finish = body + compressed;
    if (flags & 8) { const signature = archive.readUInt32LE(finish) === 0x08074b50 ? 4 : 0; finish += signature; if (finish + 12 > start || archive.readUInt32LE(finish) !== crc || archive.readUInt32LE(finish + 4) !== compressed || archive.readUInt32LE(finish + 8) !== size) fail(); finish += 12; }
    spans.push([offset, finish]);
    if (name === selected) {
      const input = archive.subarray(body, body + compressed);
      const bytes = method === 0 ? Buffer.from(input) : inflateRawSync(input, { maxOutputLength: MAX_ENTRY });
      if (bytes.length !== size || crc32(bytes) !== crc) fail();
      result = { bytes, crc32: crc, sha256: sha(bytes), size };
    }
    at += 46 + n + extra + comment;
  }
  spans.sort((a, b) => a[0] - b[0]);
  if (at !== end || spans.some((s, i) => i && spans[i - 1][1] > s[0]) || !result) fail();
  return result;
}
export function acquirePinnedSound(archive, id) {
  const name = id?.replace(/^kenney-ui-audio:/, "");
  if (!SOUNDS.has(name) || id !== `kenney-ui-audio:${name}` || archive.length !== 411949 || sha(archive) !== HASH) fail();
  const license = extractZipEntry(archive, "License.txt");
  if (license.sha256 !== LICENSE) fail();
  const result = extractZipEntry(archive, `Audio/${name}.ogg`), expected = SOUNDS.get(name);
  if (result.size !== expected.size || result.sha256 !== expected.sha256 || result.crc32 !== expected.crc32) fail();
  return result;
}
async function main() {
  let raw = "";
  for await (const chunk of process.stdin) { raw += chunk.toString("utf8"); if (Buffer.byteLength(raw) > 1024 * 1024) fail(); if (raw.endsWith("\n")) break; }
  const input = JSON.parse(raw), executionId = input.executionId;
  if (input.type !== "catalog_archive" || typeof executionId !== "string" || !/^[a-zA-Z0-9_-]{1,80}$/.test(executionId) || typeof input.archiveHex !== "string" || !/^[a-f0-9]{823898}$/.test(input.archiveHex)) fail();
  const result = acquirePinnedSound(Buffer.from(input.archiveHex, "hex"), input.catalogId);
  process.stdout.write(JSON.stringify({ executionId, type: "complete", catalogId: input.catalogId, archiveSha256: HASH, sha256: result.sha256, crc32: result.crc32, size: result.size, hex: result.bytes.toString("hex") }) + "\n");
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch(() => { process.exitCode = 1; });
