// SPDX-License-Identifier: GPL-3.0-or-later
//
// Fails when a Tauri crate in Cargo.lock and its npm package in node_modules
// are on different major/minor releases. `tauri build` refuses to start in that
// case, and CI never runs `tauri build`, so without this check the mismatch
// only shows up in the Release workflow.
//
// Run after `pnpm install`: node scripts/check-tauri-versions.mjs

import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const desktopDir = join(dirname(fileURLToPath(import.meta.url)), '..');
const cargoLockPath = join(desktopDir, '..', '..', 'Cargo.lock');

/** Every version of each crate locked in Cargo.lock, keyed by crate name. */
function lockedCrates(text) {
  const crates = new Map();
  for (const block of text.split(/^\[\[package\]\]\s*$/m).slice(1)) {
    const name = block.match(/^name = "([^"]+)"/m)?.[1];
    const version = block.match(/^version = "([^"]+)"/m)?.[1];
    if (!name || !version) continue;
    if (!crates.has(name)) crates.set(name, []);
    crates.get(name).push(version);
  }
  return crates;
}

/** The Rust crate Tauri pairs with an npm package, or null for packages it does not check. */
function crateFor(npmName) {
  if (npmName === '@tauri-apps/api') return 'tauri';
  const plugin = npmName.match(/^@tauri-apps\/plugin-(.+)$/)?.[1];
  return plugin ? `tauri-plugin-${plugin}` : null;
}

const majorMinor = (version) => version.split('.').slice(0, 2).join('.');

const pkg = JSON.parse(readFileSync(join(desktopDir, 'package.json'), 'utf8'));
const crates = lockedCrates(readFileSync(cargoLockPath, 'utf8'));
const npmNames = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });

const problems = [];
const checked = [];
for (const npmName of npmNames) {
  const crate = crateFor(npmName);
  if (!crate || !crates.has(crate)) continue;

  const installed = join(desktopDir, 'node_modules', ...npmName.split('/'), 'package.json');
  if (!existsSync(installed)) {
    problems.push(`${npmName} is not installed; run pnpm install first`);
    continue;
  }
  const npmVersion = JSON.parse(readFileSync(installed, 'utf8')).version;

  for (const crateVersion of crates.get(crate)) {
    if (majorMinor(crateVersion) === majorMinor(npmVersion)) {
      checked.push(`${crate} ${crateVersion} = ${npmName} ${npmVersion}`);
    } else {
      problems.push(`${crate} ${crateVersion} (Cargo.lock) vs ${npmName} ${npmVersion} (pnpm-lock.yaml)`);
    }
  }
}

if (problems.length > 0) {
  console.error('Tauri crates and npm packages must be on the same major/minor release:');
  for (const problem of problems) console.error(`  ${problem}`);
  console.error('Bump the npm package with `pnpm update <package>@<crate version>` in apps/desktop.');
  process.exit(1);
}

for (const line of checked) console.log(`ok  ${line}`);
