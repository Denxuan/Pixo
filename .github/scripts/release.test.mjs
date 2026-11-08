import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

test("release tag must match all project versions", () => {
  const run = (tag) => execFileSync(process.execPath, [".github/scripts/release-version.mjs"], {
    env: { ...process.env, GITHUB_REF_NAME: tag }, stdio: "pipe",
  }).toString().trim();
  assert.ok(run(`v${JSON.parse(readFileSync("package.json", "utf8")).version}`));
  assert.throws(() => run("v999.999.999"));
  assert.throws(() => run("v1.2.3-rc1"));
});

test("cask includes the actual archive checksum and release URL", () => {
  const dir = mkdtempSync(join(tmpdir(), "pixo-cask-"));
  try {
    const archive = join(dir, "package.zip");
    writeFileSync(archive, "test archive");
    const script = resolve(".github/scripts/generate-cask.mjs");
    execFileSync(process.execPath, [script, archive, "1.2.3", "owner/Pixo"], { cwd: dir });
    const cask = readFileSync(join(dir, "Casks/pixo.rb"), "utf8");
    assert.ok(cask.includes(createHash("sha256").update("test archive").digest("hex")));
    assert.ok(cask.includes("https://github.com/owner/Pixo/releases/download/v#{version}/pixo-macos-universal.zip"));
    assert.throws(() => execFileSync(process.execPath, [script, archive, "1.2.3", "invalid/repo/extra"], { cwd: dir, stdio: "pipe" }));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
