# Homebrew formula for Touring
class Touring < Formula
  desc "Code intelligence and AI-assisted refactoring"
  homepage "https://touring.dev"
  url "https://github.com/touring/touring/archive/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_FILL_AT_RELEASE"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "touring", shell_output("#{bin}/touring --version")
  end
end
