import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkRepository, scanSource } from './check-banned-apis.mjs';

describe('banned API guard', () => {
  it('flags std::fs::rename outside fsops and platform', () => {
    const violations = scanSource(
      'src-tauri/src/services/bad.rs',
      'pub fn bad() { let _ = std::fs::rename("a", "b"); }',
    );
    expect(violations).toHaveLength(1);
    expect(violations[0]?.rule).toBe('banned-std-fs-api');
  });

  it('flags imported fs::remove_file and shell-outs outside fsops and platform', () => {
    const code = [
      'use std::fs;',
      'use std::process::Command;',
      'pub fn bad() {',
      '  let _ = fs::remove_file("a");',
      '  let _ = Command::new("powershell");',
      '}',
    ].join('\n');

    const violations = scanSource('src-tauri/src/engine/bad.rs', code);
    expect(violations.map((v) => v.rule)).toEqual(['banned-std-fs-api', 'banned-shell-out']);
  });

  it('flags unsafe code outside platform/', () => {
    const violations = scanSource('src-tauri/src/commands/bad.rs', 'pub fn bad() { unsafe { } }');
    expect(violations).toHaveLength(1);
    expect(violations[0]?.rule).toBe('unsafe-outside-platform');
  });

  it('allows fs::canonicalize inside fsops/ and unsafe inside platform/', () => {
    expect(
      scanSource('src-tauri/src/fsops/sandbox.rs', 'let _ = std::fs::canonicalize("x");'),
    ).toEqual([]);
    expect(scanSource('src-tauri/src/platform/windows/mod.rs', 'pub unsafe fn win() {}')).toEqual(
      [],
    );
  });

  it('passes cleanly on the current repository source tree', () => {
    const rootDir = path.resolve(import.meta.dirname, '..');
    expect(checkRepository(rootDir)).toEqual([]);
  });
});
