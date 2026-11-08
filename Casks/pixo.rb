cask "pixo" do
  version "0.1.1"
  sha256 "064513fec43d30fd7f2b395a1f11dcb06678c9d8152d6d4d6143623cb842998b"

  url "https://github.com/Denxuan/Pixo/releases/download/v#{version}/pixo-macos-universal.zip"
  name "Pixo"
  desc "Desktop clipboard and productivity toolbox"
  homepage "https://github.com/Denxuan/Pixo"

  app "pixo.app"
end
