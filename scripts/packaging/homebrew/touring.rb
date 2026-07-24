# W14.8 partial (2026-05-23) — Homebrew Formula SKELETON for touring.
#
# Status: SKELETON — `url`/`sha256` placeholders. Activated by W13.6
# release-plz publish step + Gabriel registering a Homebrew tap at:
#   github.com/<org>/homebrew-tap
#
# Installation (when activated):
#   brew tap anthropics/touring
#   brew install touring
#
# This file lives in the workspace for review; it will be copied into the
# tap repo by the release pipeline (W14.7/W14.8 full).

class Touring < Formula
  desc "Code intelligence binary (Touring premium refactor 2026)"
  homepage "https://touring.dev"
  license "Apache-2.0 OR MIT"
  version "30.3.0"   # bumped by release-plz on each tag

  # Targets mirror the release.yml build matrix (F5, 2026-07-24). macOS Intel
  # and Linux ARM are W14 matrix-expansion territory — do not list artifacts
  # the pipeline does not produce.
  on_macos do
    on_arm do
      url "https://releases.touring.dev/30.3.0/touring-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_REPLACED_BY_RELEASE_PIPELINE"
    end
  end

  on_linux do
    on_intel do
      url "https://releases.touring.dev/30.3.0/touring-x86_64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_REPLACED_BY_RELEASE_PIPELINE"
    end
  end

  def install
    # F5 tarball layout: binaries live under bin/ (toolchain shape).
    bin.install "bin/touring"
    # Companion binaries (optional — present in full tarball)
    bin.install "bin/touring-hook" if File.exist?("bin/touring-hook")
    bin.install "bin/touring-daemon" if File.exist?("bin/touring-daemon")
  end

  def caveats
    <<~EOS
      Touring follows the rustup per-project pattern. To get started:
        touring toolchain init                  # ~/.touring/ scaffold
        cd ~/projects/myproject
        touring init-project                    # .touring/ per-project

      Full guide: https://touring.dev/getting-started
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/touring --version")
    # Confirm the rustup-pattern subcommands are present (W12.1/W12.2)
    system bin/"touring", "init-project", "--help"
    system bin/"touring", "toolchain", "list"
  end
end
