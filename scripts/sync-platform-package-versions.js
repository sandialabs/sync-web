#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const repoRoot = path.resolve(__dirname, '..');
const version = fs.readFileSync(path.join(repoRoot, 'VERSION'), 'utf8').trim();
const mode = process.argv.includes('--write') ? 'write' : 'check';
const packagePaths = [
  'services/explorer/package.json',
  'services/gateway/package.json',
  'services/gateway/ui/package.json',
  'services/workbench/package.json',
  'docs/info/package.json',
];

const formatJson = (value) => `${JSON.stringify(value, null, 2)}\n`;
let drift = false;

const syncPackage = (relativePath) => {
  const absolutePath = path.join(repoRoot, relativePath);
  const pkg = JSON.parse(fs.readFileSync(absolutePath, 'utf8'));
  if (pkg.version !== version) {
    drift = true;
    console.error(`${relativePath}: ${pkg.version} -> ${version}`);
    if (mode === 'write') {
      pkg.version = version;
      fs.writeFileSync(absolutePath, formatJson(pkg));
    }
  }

  const lockPath = path.join(path.dirname(absolutePath), 'package-lock.json');
  if (!fs.existsSync(lockPath)) {
    return;
  }
  const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
  let changed = false;
  if (lock.version && lock.version !== version) {
    drift = true;
    changed = true;
    console.error(`${path.relative(repoRoot, lockPath)}: root version ${lock.version} -> ${version}`);
    if (mode === 'write') {
      lock.version = version;
    }
  }
  if (lock.packages && lock.packages[''] && lock.packages[''].version !== version) {
    drift = true;
    changed = true;
    console.error(`${path.relative(repoRoot, lockPath)}: packages[""] ${lock.packages[''].version} -> ${version}`);
    if (mode === 'write') {
      lock.packages[''].version = version;
    }
  }
  if (mode === 'write' && changed) {
    fs.writeFileSync(lockPath, formatJson(lock));
  }
};

for (const packagePath of packagePaths) {
  syncPackage(packagePath);
}

if (drift && mode === 'check') {
  console.error(`Package versions are not synced to platform VERSION ${version}. Run: node scripts/sync-platform-package-versions.js --write`);
  process.exit(1);
}

if (!drift) {
  console.log(`Package versions already match platform VERSION ${version}.`);
}
