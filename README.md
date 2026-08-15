# FerriOS

A toy OS in Rust🦀, based on blog\_os from [Writing an OS in Rust](https://os.phil-opp.com) and inspired by [xv6](https://github.com/mit-pdos/xv6-riscv).

## Background

[blog\_os](https://github.com/phil-opp/blog_os) is an excellent blog series that walks through OS development in Rust with remarkable clarity — it doesn't just show *what* to implement, but explains *why* each piece works the way it does. However, the series currently ends at the Async/Await chapter, leaving higher-level OS features uncharted.

FerriOS starts where blog_os leaves off. Taking blog_os as its foundation and [xv6](https://github.com/mit-pdos/xv6-riscv) as a reference for Unix-like OS design, the goal is to build a fully functional OS that reaches — and eventually exceeds — xv6 in capability, while staying true to Rust's strengths: memory safety, expressive type system, and fearless concurrency.

**Note:** FerriOS is a personal hobby project and is not intended as a learning resource or tutorial. While the code is publicly available, please keep in mind that it comes with no guarantees of correctness, completeness, or instructional value.

# Preparation

Install nightly toolchain, rust-src, and llvm-tools-preview

```bash
$ rustup toolchain install nightly --profile minimal --component rust-src --component llvm-tools-preview --target x86_64-unknown-none
$ rustup override set nightly
```

Install llvm-tools-preview component

```bash
$ rustup component add llvm-tools-preview
```

# Build
```bash
$ cargo build --release
```

# Run
Run in QEMU with graphical output
```bash
$ cargo run --release
```

Run in QEMU with no graphical output (serial console only)
```bash
$ cargo run --release -- --nographic
```
