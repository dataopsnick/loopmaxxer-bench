# SKILL: Rust CI/CD with GitHub Actions

## Overview
Implementing Continuous Integration and Continuous Delivery (CI/CD) for Rust projects using GitHub Actions automates testing, formatting, and linting. This ensures high code quality, catches regressions early, and guarantees cross-platform deployability while saving manual build time.

## Prerequisites
* A standard Rust project (e.g., created via `cargo new`).
* The project hosted in a GitHub repository.

## Core Benefits for Rust Development
* **Catch Regressions Early:** Automated test runs on every push/PR.
* **Maintain Code Quality:** Enforce stylistic consistency and catch common mistakes using `rustfmt` and `clippy`.
* **Cross-Platform Validation:** Seamlessly verify compilation across Linux, macOS, and Windows.
* **Fast Feedback Loops:** Utilize caching and optimized runners to significantly cut down standard compile times.

---

## The Comprehensive Workflow Template

Create a workflow file in your repository at `.github/workflows/rust-ci.yml`. The following template includes speed optimizations, advanced testing, and cross-platform matrix testing.

```yaml
# .github/workflows/rust-ci.yml
name: Rust CI 🚀

on:
  push:
    branches: [ "main", "develop" ]
  pull_request:
    branches: [ "main", "develop" ]

jobs:
  build_and_test:
    name: Build and Test on ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        # Cross-platform testing matrix
        os: [ubuntu-latest, macos-latest, windows-latest] 
        
    env:
      CARGO_TERM_COLOR: always
      # CI Optimizations:
      CARGO_INCREMENTAL: 0         # Disable incremental compilation (faster for fresh CI builds)
      CARGO_PROFILE_DEV_DEBUG: 0   # Strip debug info from 'dev' profile
      CARGO_PROFILE_TEST_DEBUG: 0  # Strip debug info from 'test' profile

    steps:
      - name: ⬇️ Checkout code
        uses: actions/checkout@v6

      - name: ⚙️ Setup Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: rustfmt, clippy

      - name: 📦 Restore Cargo cache
        uses: Swatinem/rust-cache@v2.8.0

      - name: 📥 Install cargo-nextest
        uses: taiki-e/install-action@v2
        with:
          tool: nextest

      - name: 📝 Check code formatting with rustfmt
        run: cargo fmt --all --check

      - name: 🔍 Lint code with Clippy
        # -D warnings treats all warnings as errors, failing the build
        run: cargo clippy -- -D warnings 

      - name: 🏗️ Build project
        run: cargo build --verbose

      - name: ✅ Run unit & integration tests (cargo-nextest)
        run: cargo nextest run --verbose

      - name: 📄 Run Doctests (cargo-nextest fallback)
        # nextest does not support doctests yet, so we run them with standard cargo
        run: cargo test --doc
        continue-on-error: true
```

---

## Key GitHub Actions Used

1. **`actions/checkout@v6`**: Pulls the repository code into the CI runner.
2. **`dtolnay/rust-toolchain@stable`**: Installs the specified Rust toolchain. Allows you to strictly define components (like `rustfmt` and `clippy`) avoiding manual `rustup` setup.
3. **`Swatinem/rust-cache@v2.8.0`**: Intelligently caches the `~/.cargo` and `./target` directories to persist installed binaries, registries, and build artifacts between runs. 
4. **`taiki-e/install-action@v2`**: A utility to install CLI tools—used here to instantly download pre-built binaries of `cargo-nextest`.

---

## Performance Tuning Details

Rust compile times can be lengthy, especially on fresh CI runners. Use these specific flags to optimize your workflow:

* **Disable Incremental Compilation (`CARGO_INCREMENTAL=0`)**: Incremental compilation uses extra I/O and CPU to save intermediate state. In ephemeral CI environments, compiling from scratch without tracking incremental state is faster.
* **Disable Debug Info (`CARGO_PROFILE_*_DEBUG=0`)**: Generating debug symbols drastically increases artifact size and compilation time. Usually unnecessary for standard pass/fail CI tests.
* **Use `cargo-nextest`**: A next-generation test runner for Rust. It isolates tests in separate processes and runs them in parallel, resulting in massive speedups over standard `cargo test`.
    * *Note:* `cargo-nextest` does not currently support Rust doctests. Always add a dedicated `cargo test --doc` step if your project relies on documentation tests.

---

## Next Steps: Deployment & Releases (CD)

Once your CI pipeline is consistently passing, consider expanding your workflows to handle delivery:

* **Automated GitHub Releases**: Use actions like `softprops/action-gh-release` to attach cross-compiled binaries to Git tags automatically.
* **Publish to Crates.io**: Automate `cargo publish` triggering on version tags (e.g., `on: push: tags: ['v*.*.*']`).
* **Cloud Deployments**: Add steps to build Docker images and push them to registries (Docker Hub, AWS ECR) or deploy directly to services like Kubernetes or AWS Lambda upon a merge to `main`.