import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const BANNED_FS_CALLS = [
  'rename',
  'copy',
  'remove_file',
  'remove_dir',
  'remove_dir_all',
  'write',
  'canonicalize',
];

const STD_FS_PATTERN = new RegExp(`\\bstd::fs::(?:${BANNED_FS_CALLS.join('|')})\\b`);
const IMPORTED_FS_PATTERN = new RegExp(`\\bfs::(?:${BANNED_FS_CALLS.join('|')})\\b`);
const SHELL_OUT_PATTERN =
  /Command::new\(\s*"(?:cmd|cmd\.exe|powershell|powershell\.exe|pwsh|pwsh\.exe|robocopy|robocopy\.exe|explorer|explorer\.exe)"/i;
const UNSAFE_CODE_PATTERN = /\bunsafe\s*\{|\bunsafe\s+fn\b/;

/**
 * Scans a single Rust source file for banned filesystem, shell-out, or unsafe patterns.
 * Returns an array of violation objects.
 */
export function scanSource(relativePath, content) {
  const normalized = relativePath.replace(/\\/g, '/');
  const inFsopsOrPlatform =
    normalized.includes('src-tauri/src/fsops/') || normalized.includes('src-tauri/src/platform/');
  const inPlatform = normalized.includes('src-tauri/src/platform/');
  const isLibRs = normalized.endsWith('src-tauri/src/lib.rs');

  const lines = content.split(/\r?\n/);
  const importsStdFs = /\buse\s+std::fs\b/.test(content);
  const violations = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    const lineNo = i + 1;

    if (!inFsopsOrPlatform) {
      if (STD_FS_PATTERN.test(line) || (importsStdFs && IMPORTED_FS_PATTERN.test(line))) {
        violations.push({
          file: normalized,
          line: lineNo,
          rule: 'banned-std-fs-api',
          snippet: line.trim(),
        });
      }
    }

    if (SHELL_OUT_PATTERN.test(line)) {
      violations.push({
        file: normalized,
        line: lineNo,
        rule: 'banned-shell-out',
        snippet: line.trim(),
      });
    }

    if (!inPlatform) {
      if (UNSAFE_CODE_PATTERN.test(line)) {
        violations.push({
          file: normalized,
          line: lineNo,
          rule: 'unsafe-outside-platform',
          snippet: line.trim(),
        });
      }

      if (line.includes('#[allow(unsafe_code)]')) {
        const nextLine = (lines[i + 1] ?? '').trim();
        const isAllowedPlatformModDecl = isLibRs && nextLine === 'pub mod platform;';
        if (!isAllowedPlatformModDecl) {
          violations.push({
            file: normalized,
            line: lineNo,
            rule: 'allow-unsafe-outside-platform',
            snippet: line.trim(),
          });
        }
      }
    }
  }

  return violations;
}

function walkRustFiles(dir) {
  const results = [];
  if (!fs.existsSync(dir)) {
    return results;
  }
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...walkRustFiles(full));
    } else if (entry.isFile() && entry.name.endsWith('.rs')) {
      results.push(full);
    }
  }
  return results;
}

export function checkRepository(rootDir) {
  const srcDir = path.join(rootDir, 'src-tauri', 'src');
  const files = walkRustFiles(srcDir);
  const allViolations = [];
  for (const file of files) {
    const rel = path.relative(rootDir, file);
    const content = fs.readFileSync(file, 'utf8');
    allViolations.push(...scanSource(rel, content));
  }
  return allViolations;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const violations = checkRepository(rootDir);
  if (violations.length > 0) {
    console.error('Banned API guard failed with violations:');
    for (const v of violations) {
      console.error(`  ${v.file}:${v.line} [${v.rule}] ${v.snippet}`);
    }
    process.exit(1);
  }
  console.log('Banned API guard passed.');
}
