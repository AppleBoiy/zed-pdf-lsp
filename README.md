# zed-pdf-lsp

A Language Server Protocol (LSP) server that enables the [Zed editor](https://zed.dev) to display PDF files by converting them to Markdown. When you open a PDF in Zed, the server extracts text content and presents it as readable Markdown.

## Features

- PDF text extraction with paragraph and heading preservation
- Multi-page PDF support with sequential page rendering
- Error handling for corrupted, encrypted, and image-only PDFs
- Async, non-blocking conversion with 10-second timeout
- Memory limit enforcement (500MB per conversion)
- Structured logging via `tracing`

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.70+)
- [Zed editor](https://zed.dev) (for editor integration)

## Building

```sh
cargo build --release
```

The binary will be at `target/release/zed-pdf-lsp`.

## Running Tests

```sh
cargo test
```

This runs 144 tests including unit tests, property-based tests (via `proptest`), integration tests, and performance benchmarks.

## Manual Testing

A test script is included to send raw LSP messages to the server via stdin/stdout:

```sh
chmod +x test.sh
./test.sh
```

This sends an initialize handshake followed by a `textDocument/didOpen` for `example.pdf`, and prints the server's JSON-RPC responses. Edit `test.sh` to change the PDF path.

## Configuring Zed

> **Note:** Zed requires a [language extension](https://zed.dev/docs/extensions) to register custom LSP servers. The `lsp` settings key only accepts known server names. A Zed extension for zed-pdf-lsp is planned.

For now, you can test the server standalone using the `test.sh` script or by piping JSON-RPC messages directly.

## Usage

1. Build the server: `cargo build --release`
2. Run the test script: `./test.sh`
3. The server reads `example.pdf`, converts it to Markdown, and outputs the result as JSON-RPC messages.

## How It Works

The server communicates over stdin/stdout using JSON-RPC 2.0 (the LSP transport). When it receives a `textDocument/didOpen` notification for a `.pdf` file, it:

1. Reads the PDF binary from disk.
2. Extracts text page-by-page using `pdf-extract`.
3. Detects headings (all-caps, numbered sections, "Chapter" prefixes).
4. Formats the text as Markdown with headings and page separators (`---`).
5. Sends the Markdown content back to the client.

## Architecture

```
src/
├── main.rs              # Entry point, tokio runtime, tracing setup
├── lib.rs               # Module declarations
├── server.rs            # LSP server core (tower-lsp LanguageServer impl)
├── message_handler.rs   # Request/response logging, error formatting
├── pdf_converter.rs     # PDF text extraction and Markdown conversion
└── document_registry.rs # Thread-safe registry of open documents
```

## Error Handling

The server returns Markdown-formatted error messages for:
- File not found / not readable
- Corrupted PDF structure
- Encrypted/password-protected PDFs
- Empty or image-only PDFs
- Memory limit exceeded
- Conversion timeout

## Logging

Logs are written to stderr. Control the log level with `RUST_LOG`:

```sh
RUST_LOG=debug ./target/release/zed-pdf-lsp
```

## License

MIT
