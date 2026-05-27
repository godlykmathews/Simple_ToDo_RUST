# Simple Todo Rust

A simple, fast, and lightweight Todo application written in Rust and powered by the [Slint](https://slint.dev/) UI framework.

## Features
- Add, remove, and manage daily tasks easily.
- Fast and responsive graphical user interface.
- Lightweight binary, compiled to native code.

## Prerequisites

To build and run this project, you will need:
- [Rust & Cargo](https://rustup.rs/)

## Building from Source

To compile the application for your current platform, run:

```bash
cargo build --release
```

The optimized binary will be located at `target/release/simple_todo_rust`.

## Running the Application

During development, you can run the application directly with:

```bash
cargo run
```

Or run the release version:

```bash
cargo run --release
```

## Releasing / Packaging

When building for a production release, it is recommended to use the `--release` flag to ensure the binary is optimized for speed and size.

```bash
cargo build --release
```
