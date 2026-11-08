import { readFileSync } from "node:fs";

const tag = process.env.GITHUB_REF_NAME;
if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? "")) {
  throw new Error("Release tags must have the form v1.2.3");
}
const version = tag.slice(1);
const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargo.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
if ([packageVersion, tauriVersion, cargoVersion].some((value) => value !== version)) {
  throw new Error(`Tag ${tag} must match package.json, tauri.conf.json and Cargo.toml versions`);
}
console.log(version);
