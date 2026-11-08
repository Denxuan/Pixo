import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

const [archive, version, repository] = process.argv.slice(2);
if (!archive || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version ?? "") || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? "")) {
  throw new Error("Usage: node generate-cask.mjs <archive> <version> <owner/repository>");
}
const sha = createHash("sha256").update(readFileSync(archive)).digest("hex");
const cask = `cask "pixo" do
  version "${version}"
  sha256 "${sha}"

  url "https://github.com/${repository}/releases/download/v#{version}/pixo-macos-universal.dmg"
  name "Pixo"
  desc "Desktop clipboard and productivity toolbox"
  homepage "https://github.com/${repository}"

  app "pixo.app"
end
`;
mkdirSync("Casks", { recursive: true });
writeFileSync("Casks/pixo.rb", cask);
