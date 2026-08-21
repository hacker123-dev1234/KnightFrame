// 列出 PE 的全部导入（DLL + 函数名），并与实际 DLL 的导出表比对，报告缺失入口
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const file = process.argv[2];
if (!file) process.exit(2);
const b = readFileSync(file);

const pe = b.readUInt32LE(0x3c);
const machine = b.readUInt16LE(pe + 4);
const opt = pe + 0x18;
const is64 = b.readUInt16LE(pe + 0x18) === 0x20b;
const dataDir = opt + (is64 ? 112 : 96);
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
function cstr(buf, off) {
  let end = off;
  while (buf[end] !== 0) end += 1;
  return buf.toString('ascii', off, end);
}

function imports(buf) {
  const p = buf.readUInt32LE(0x3c);
  const o = p + 0x18;
  const d64 = buf.readUInt16LE(p + 0x18) === 0x20b;
  const dir = o + (d64 ? 112 : 96);
  const rva = buf.readUInt32LE(dir + 8);
  const n = buf.readUInt16LE(p + 6);
  const so = o + buf.readUInt16LE(p + 0x14);
  const r2o = (r) => {
    for (let i = 0; i < n; i += 1) {
      const s = so + i * 40;
      const vs = buf.readUInt32LE(s + 8);
      const va = buf.readUInt32LE(s + 12);
      const rw = buf.readUInt32LE(s + 20);
      if (r >= va && r < va + vs) return rw + (r - va);
    }
    return -1;
  };
  const out = [];
  let d = r2o(rva);
  while (d >= 0 && d + 20 <= buf.length) {
    const nameRva = buf.readUInt32LE(d + 12);
    if (nameRva === 0) break;
    const dll = cstr(buf, r2o(nameRva));
    const intRva = buf.readUInt32LE(d + 0) || buf.readUInt32LE(d + 16);
    const names = [];
    let t = r2o(intRva);
    while (t >= 0 && t + 8 <= buf.length) {
      const v = buf.readBigUInt64LE(t);
      if (v === 0n) break;
      if ((v & 0x8000000000000000n) === 0n) {
        const no = r2o(Number(v & 0xffffffffn));
        if (no >= 0) names.push(cstr(buf, no + 2));
      } else {
        names.push(`#ordinal${Number(v & 0xffffn)}`);
      }
      t += 8;
    }
    out.push({ dll, names });
    d += 20;
  }
  return out;
}

function exportsOf(buf) {
  const p = buf.readUInt32LE(0x3c);
  const o = p + 0x18;
  const d64 = buf.readUInt16LE(p + 0x18) === 0x20b;
  const dir = o + (d64 ? 112 : 96);
  const expRva = buf.readUInt32LE(dir);
  const n = buf.readUInt16LE(p + 6);
  const so = o + buf.readUInt16LE(p + 0x14);
  const r2o = (r) => {
    for (let i = 0; i < n; i += 1) {
      const s = so + i * 40;
      const vs = buf.readUInt32LE(s + 8);
      const va = buf.readUInt32LE(s + 12);
      const rw = buf.readUInt32LE(s + 20);
      if (r >= va && r < va + vs) return rw + (r - va);
    }
    return -1;
  };
  if (expRva === 0) return new Set();
  const e = r2o(expRva);
  const namesRva = buf.readUInt32LE(e + 32);
  const numNames = buf.readUInt32LE(e + 24);
  const set = new Set();
  const no = r2o(namesRva);
  for (let i = 0; i < numNames; i += 1) {
    set.add(cstr(buf, r2o(buf.readUInt32LE(no + i * 4))));
  }
  return set;
}

const searchDirs = [
  'C:/Windows/System32',
  'C:/Windows/SysWOW64',
  'd:/Game/Minecraft/Eps+/Epsilon-26.1.x/knightframe-rs/src-tauri/target/debug/deps',
];
function findDll(name) {
  for (const dir of searchDirs) {
    const p = join(dir, name);
    if (existsSync(p)) return p;
  }
  return undefined;
}

const machineName = { 0x8664: 'x64', 0xaa64: 'arm64', 0x14c: 'x86' }[machine] ?? `0x${machine.toString(16)}`;
console.log(`${file} machine=${machineName}`);
let problems = 0;
for (const { dll, names } of imports(b)) {
  const path = findDll(dll);
  if (!path) {
    console.log(`  [NOT FOUND] ${dll} (${names.length} funcs)`);
    problems += 1;
    continue;
  }
  const sys = readFileSync(path);
  const sysMachine = sys.readUInt16LE(sys.readUInt32LE(0x3c) + 4);
  if (sysMachine !== machine) {
    console.log(`  [ARCH MISMATCH] ${dll}: exe=${machineName} dll=${{ 0x8664: 'x64', 0xaa64: 'arm64', 0x14c: 'x86' }[sysMachine] ?? sysMachine} (${path})`);
    problems += 1;
    continue;
  }
  const exp = exportsOf(sys);
  const missing = names.filter((fn) => !fn.startsWith('#ordinal') && !exp.has(fn));
  if (missing.length) {
    console.log(`  [MISSING ENTRYPOINTS] ${dll} <- ${path}`);
    for (const fn of missing) console.log(`      ${fn}`);
    problems += 1;
  } else {
    console.log(`  ok ${dll} (${names.length} funcs)`);
  }
}
console.log(problems ? `PROBLEMS=${problems}` : 'ALL IMPORTS RESOLVED');
