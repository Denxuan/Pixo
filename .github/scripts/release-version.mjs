import { readFileSync, writeFileSync } from "node:fs";

const tag = process.env.GITHUB_REF_NAME;
if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? "")) {
  throw new Error("Release tags must have the form v1.2.3");
}
const version = tag.slice(1);

function stamp(path, pattern) {
  const original = readFileSync(path, "utf8");
  if (!pattern.test(original)) throw new Error(`Could not find a version to update in ${path}`);
  const updated = original.replace(pattern, `$1${version}$2`);
  writeFileSync(path, updated);
}

stamp("package.json", /("version"\s*:\s*")[^"]+(")/);
stamp("src-tauri/tauri.conf.json", /("version"\s*:\s*")[^"]+(")/);
stamp("src-tauri/Cargo.toml", /^(\[package\][\s\S]*?^version\s*=\s*")[^"]+(")/m);
stamp("src-tauri/Cargo.lock", /(name = "pixo"\nversion = ")[^"]+(")/);
console.log(version);
