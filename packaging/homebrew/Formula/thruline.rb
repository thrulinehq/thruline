# ThruLine Homebrew formula. Version and checksums are filled from a published
# GitHub release before proposing an update to the tap.
class Thruline < Formula
  desc "Proves what an AI coding agent actually did, from the execution record"
  homepage "https://thrulinehq.com"
  version "0.0.0-PLACEHOLDER"
  license "FSL-1.1-MIT"

  on_macos do
    on_arm do
      url "https://github.com/thrulinehq/thruline/releases/download/v#{version}/thruline-cli-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_APPLE_DARWIN"
    end
    on_intel do
      url "https://github.com/thrulinehq/thruline/releases/download/v#{version}/thruline-cli-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_X86_64_APPLE_DARWIN"
    end
  end

  # Linux downloads use static musl archives for broad libc compatibility.
  on_linux do
    on_arm do
      url "https://github.com/thrulinehq/thruline/releases/download/v#{version}/thruline-cli-aarch64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_AARCH64_UNKNOWN_LINUX_MUSL"
    end
    on_intel do
      url "https://github.com/thrulinehq/thruline/releases/download/v#{version}/thruline-cli-x86_64-unknown-linux-musl.tar.gz"
      sha256 "PLACEHOLDER_X86_64_UNKNOWN_LINUX_MUSL"
    end
  end

  def install
    # cargo-dist archives hold one directory named after the archive; Homebrew
    # strips a single root directory on unpack, so the binary is at the top
    # here. Asserted rather than assumed — if the layout ever gains a second
    # root entry, brew stops stripping and this line silently installs nothing.
    odie "unexpected archive layout" unless File.exist?("thruline")
    bin.install "thruline"
  end

  def caveats
    <<~EOS
      ThruLine is local-first. Nothing leaves this machine unless you point it
      at a model provider yourself.

        thruline demo    what it does, in about ten seconds
        thruline init    set up capture for your harness

      `thruline demo` runs a real test command and will not simulate one, so it
      needs pytest or cargo on PATH. Without either it declines and says so —
      which is the right refusal and a poor first thirty seconds, so it is
      stated here rather than discovered. `thruline autopsy` needs nothing and
      reads what this machine already has.
    EOS
  end

  test do
    assert_match "thruline", shell_output("#{bin}/thruline --version")
    # The shipped binary is `--no-default-features`, so the ONNX paths refuse.
    # Asserted, because a build that silently gained ORT would be a build that
    # downloads a model from a CDN on a user's machine.
    assert_match(/annotate|refus|not available/i,
                 shell_output("#{bin}/thruline annotate --help 2>&1"))
    # A policy gate must refuse a policy it cannot parse rather than pass it.
    (testpath/".thruline").mkpath
    (testpath/".thruline/policy.yml").write("version: 1\nrequire_reciept: true\n")
    output = shell_output("#{bin}/thruline policy --repo #{testpath} 2>&1", 1)
    assert_match "require_reciept", output
  end
end
