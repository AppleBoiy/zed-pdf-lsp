# zed-pdf-lsp

A Language Server Protocol (LSP) server that enables the [Zed editor](https://zed.dev) to display PDF files by converting them to Markdown. When you open a PDF in Zed, the server extracts text content and presents it as readable Markdown.

## Features

- PDF text extraction with paragraph and heading preservation
- Multi-page PDF support with sequential page rendering
- Error handling for corrupted, encrypted, and image-only PDFs
- Async, non-blocking conversion
- Structured logging via `tracing`

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.70+)
- [Zed editor](https://zed.dev)

## Building

```sh
# Clone the repository
git clone https://github.com/your-org/zed-pdf-lsp.git
cd zed-pdf-lsp

# Build in release mode
cargo build --release
```

The binary will be at `target/release/zed-pdf-lsp`.

### Running Tests

```sh
cargo test
```

## Configuring Zed

Add the following to your Zed settings file (`~/.config/zed/settings.json` on Linux, `~/Library/Application Support/Zed/settings.json` on macOS):

```json
{
  "lsp": {
    "zed-pdf-lsp": {
      "binary": {
        "path": "/absolute/path/to/zed-pdf-lsp/target/release/zed-pdf-lsp"
      }
    }
  },
  "file_types": {
    "pdf": ["pdf"]
  },
  "languages": {
    "pdf": {
      "language_servers": ["zed-pdf-lsp"]
    }
  }
}
```

Replace `/absolute/path/to/zed-pdf-lsp/target/release/zed-pdf-lsp` with the actual path to your built binary.

## Usage

1. Build the server and configure Zed as described above.
2. Open (or restart) Zed.
3. Open any `.pdf` file from the file tree or via the file picker.
4. The PDF content appears as Markdown in the editor.

## How It Works

The server communicates with Zed over stdin/stdout using JSON-RPC 2.0 (the LSP transport). When Zed sends a `textDocument/didOpen` notification for a `.pdf` file, the server:

1. Reads the PDF binary from disk.
2. Extracts text page-by-page using `pdf-extract`.
3. Formats the text as Markdown with headings and page separators.
4. Sends the Markdown content back to Zed.

## Logging

Logs are written to stderr so they don't interfere with the JSON-RPC transport on stdout. Control the log level with the `RUST_LOG` environment variable:

```sh
RUST_LOG=debug /path/to/zed-pdf-lsp
```

## License

MIT
