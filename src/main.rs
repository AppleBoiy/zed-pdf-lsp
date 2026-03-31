mod document_registry;
mod message_handler;
mod pdf_converter;
mod server;

use tower_lsp::{LspService, Server};

use crate::server::PdfLspServer;

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber for structured logging (Requirements 6.4, 6.5)
    // Log format: [timestamp] [level] [component] message key=value
    // Default level is INFO; set RUST_LOG=debug for development debugging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(true) // Include component/module name in log output
        .with_level(true) // Include log level
        .with_writer(std::io::stderr) // stderr so stdout stays free for JSON-RPC
        .init();

    tracing::info!("Starting zed-pdf-lsp server");

    // Configure LspService with PdfLspServer (Task 8.1)
    // tower-lsp handles Content-Length header framing automatically (Requirement 5.4)
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(PdfLspServer::new);

    // Start LSP server with stdin/stdout transport (Requirement 5.1)
    tracing::info!("zed-pdf-lsp server listening on stdin/stdout");
    Server::new(stdin, stdout, socket).serve(service).await;

    tracing::info!("zed-pdf-lsp server shut down");
}
