// Copies the canonical game data (../data) into Vite's served public/data dir.
// Run automatically before `dev` and `build` (npm pre-scripts). public/data is
// generated output and gitignored — never hand-edit it; edit ../data instead.
import { cp, rm, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const src = resolve(here, '../../data');
const dest = resolve(here, '../public/data');

await rm(dest, { recursive: true, force: true });
await mkdir(dest, { recursive: true });
await cp(src, dest, { recursive: true });
console.log(`[sync-data] ${src} -> ${dest}`);
