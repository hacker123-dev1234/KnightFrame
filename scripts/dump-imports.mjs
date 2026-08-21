// 解析 PE 导入表，列出 exe/dll 的依赖 DLL 清单
import { readFileSync } from 'node:fs';

const file = process.argv[2];
if (!file) {
  console.error('usage: node dump-imports.mjs <pe-file>');
  process.exit(2);
}
const b = readFileSync(file);
const pe = b.readUInt32LE(0x3c);
const machine = b.readUInt16LE(pe + 4);
const magic = b.readUInt16LE(pe + 0x18);
const opt = pe + 0x18;
const is64 = magic === 0x20b;
const dataDir = opt + (is64 ? 112 : 96);
const importRva = b.readUInt32LE(dataDir + 8);
const numSections = b.readUInt16LE(pe + 6);
const secOff = opt + b.readUInt16LE(pe + 0x14);

function rvaToOff(rva) {
  for (let i = 0; i < numSections; i += 1) {
    const s = secOff + i * 40;
    const vsize = b.readUInt32LE(s + 8);
    const vaddr = b.readUInt32LE(s + 12);
    const raw = b.readUInt32LE(s + 20);
    if (rva >= vaddr && rva < vaddr + vsize) return raw + (rva - vaddr);
  }
  return -1;
}
function cstr(off) {
  let end = off;
  while (b[end] !== 0) end += 1;
  return b.toString('ascii', off, end);
}

const machineName = { 0x8664: 'x64', 0xaa64: 'arm64', 0x14c: 'x86' }[machine] ?? `0x${machine.toString(16)}`;
console.log(`${file} machine=${machineName}`);
if (importRva === 0) {
  console.log('  no imports');
} else {
  let desc = rvaToOff(importRva);
for (;;) {
  if (desc < 0 || desc + 20 > b.length) { console.log('  (import descriptor out of range)'); break; }
  const nameRva = b.readUInt32LE(desc + 12);
  if (nameRva === 0) break;
  const nameOff = rvaToOff(nameRva);
  if (nameOff < 0) { console.log('  (name rva unresolved)'); break; }
  const dll = cstr(nameOff);
  const thunk = rvaToOff(b.readUInt32LE(desc + 16));
  let count = 0;
  if (thunk >= 0) {
    let t = thunk;
    while (t + 8 <= b.length && Number(b.readBigUInt64LE(t)) !== 0n) { count += 1; t += 8; }
  }
  console.log(`  ${dll} (${count} funcs)`);
  desc += 20;
}
}
