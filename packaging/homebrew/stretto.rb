# A Homebrew formula for stretto's release binaries, as a template: the
# release workflow fills in the version and the archives' checksums with
# packaging/homebrew/fill.sh and attaches the result to the release as
# stretto.rb. Homebrew installs it from a tap, a repository named
# homebrew-tap under the same account, as Formula/stretto.rb; that
# repository does not exist yet (docs/releasing.md).
class Stretto < Formula
  desc "Learn an LLM agent's next reads from its tool calls and serve them over MCP"
  homepage "https://alexnodeland.github.io/stretto/"
  version "@VERSION@"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/alexnodeland/stretto/releases/download/v#{version}/stretto-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_AARCH64_APPLE_DARWIN@"
    end
    on_intel do
      url "https://github.com/alexnodeland/stretto/releases/download/v#{version}/stretto-x86_64-apple-darwin.tar.gz"
      sha256 "@SHA256_X86_64_APPLE_DARWIN@"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/alexnodeland/stretto/releases/download/v#{version}/stretto-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_AARCH64_UNKNOWN_LINUX_GNU@"
    end
    on_intel do
      url "https://github.com/alexnodeland/stretto/releases/download/v#{version}/stretto-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_X86_64_UNKNOWN_LINUX_GNU@"
    end
  end

  def install
    bin.install "stretto", "stretto-proxy", "stretto-procedure", "stretto-mcp-demo", "stretto-console"
    generate_completions_from_executable(bin/"stretto", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/stretto --version")
    assert_match version.to_s, shell_output("#{bin}/stretto-proxy --version")
  end
end
