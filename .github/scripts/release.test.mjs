import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

test("release tag writes its version into the project files", () => {
  const dir = mkdtempSync(join(tmpdir(), "pixo-version-"));
  try {
    const tauriDir = join(dir, "src-tauri");
    mkdirSync(tauriDir);
    writeFileSync(join(dir, "package.json"), '{\n  "name": "pixo",\n  "version": "0.0.1"\n}\n');
    writeFileSync(join(tauriDir, "tauri.conf.json"), '{\n  "productName": "pixo",\n  "version": "0.0.1"\n}\n');
    writeFileSync(join(tauriDir, "Cargo.toml"), '[package]\nname = "pixo"\nversion = "0.0.1"\n\n[dependencies]\nserde = { version = "1" }\n');
    writeFileSync(join(tauriDir, "Cargo.lock"), '[[package]]\nname = "pixo"\nversion = "0.0.1"\n');
    const script = resolve(".github/scripts/release-version.mjs");
    const run = (tag) => execFileSync(process.execPath, [script], {
      cwd: dir, env: { ...process.env, GITHUB_REF_NAME: tag }, stdio: "pipe",
    }).toString().trim();
    assert.equal(run("v1.4.0"), "1.4.0");
    assert.equal(JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version, "1.4.0");
    assert.equal(JSON.parse(readFileSync(join(tauriDir, "tauri.conf.json"), "utf8")).version, "1.4.0");
    assert.match(readFileSync(join(tauriDir, "Cargo.toml"), "utf8"), /\[package\][\s\S]*?^version = "1\.4\.0"/m);
    assert.match(readFileSync(join(tauriDir, "Cargo.toml"), "utf8"), /serde = \{ version = "1" \}/);
    assert.match(readFileSync(join(tauriDir, "Cargo.lock"), "utf8"), /name = "pixo"\nversion = "1\.4\.0"/);
    assert.throws(() => run("v1.2.3-rc1"));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("cask includes the actual archive checksum and release URL", () => {
  const dir = mkdtempSync(join(tmpdir(), "pixo-cask-"));
  try {
    const archive = join(dir, "package.dmg");
    writeFileSync(archive, "test archive");
    const script = resolve(".github/scripts/generate-cask.mjs");
    execFileSync(process.execPath, [script, archive, "1.2.3", "owner/Pixo"], { cwd: dir });
    const cask = readFileSync(join(dir, "Casks/pixo.rb"), "utf8");
    assert.ok(cask.includes(createHash("sha256").update("test archive").digest("hex")));
    assert.ok(cask.includes("https://github.com/owner/Pixo/releases/download/v#{version}/pixo-macos-universal.dmg"));
    assert.throws(() => execFileSync(process.execPath, [script, archive, "1.2.3", "invalid/repo/extra"], { cwd: dir, stdio: "pipe" }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
