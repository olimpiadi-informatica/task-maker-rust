class TaskMakerRust < Formula
  desc "The new cmsMake"
  homepage "https://github.com/edomora97/task-maker-rust"

  depends_on "rust" => :build

  url "https://github.com/edomora97/task-maker-rust/archive/ARCHIVE_VERSION.tar.gz"
  sha256 "ARCHIVE_SHA256"

  def install
    ENV["TM_DATA_DIR"] = share

    system "cargo", "build", "--release", "--bin", "task-maker"
    system "cargo", "run", "--release", "--bin", "task-maker-tools", "gen-autocompletion"

    mv "target/release/task-maker", "target/release/task-maker-rust"
    bin.install "target/release/task-maker-rust"
    share.install Dir["data/*"]

    bash_completion.install "target/autocompletion/task-maker-rust.bash"
    fish_completion.install "target/autocompletion/task-maker-rust.fish"
    zsh_completion.install "target/autocompletion/_task-maker-rust"
  end
end
