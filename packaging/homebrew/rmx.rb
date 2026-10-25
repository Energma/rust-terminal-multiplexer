# Homebrew formula template. Fill in OWNER and the sha256 values from the
# release's .sha256 files, then put this in a tap (e.g. OWNER/homebrew-tap).
class Rmx < Formula
  desc "Cross-platform terminal multiplexer with menus and custom keybindings"
  homepage "https://github.com/OWNER/rust-multiplexer"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/OWNER/rust-multiplexer/releases/download/v#{version}/rmx-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "https://github.com/OWNER/rust-multiplexer/releases/download/v#{version}/rmx-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  on_linux do
    url "https://github.com/OWNER/rust-multiplexer/releases/download/v#{version}/rmx-v#{version}-x86_64-unknown-linux-musl.tar.gz"
    sha256 "REPLACE_WITH_SHA256"
  end

  def install
    bin.install "rmx"
  end

  test do
    assert_match "rmx", shell_output("#{bin}/rmx --version")
  end
end
