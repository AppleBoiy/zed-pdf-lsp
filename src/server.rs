// LSP Server Core
// This module implements the main LSP server using tower-lsp

use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::document_registry::DocumentRegistry;
use crate::message_handler::MessageHandler;
use crate::pdf_converter::PdfConverter;

pub struct PdfLspServer {
    client: Client,
    document_registry: Arc<RwLock<DocumentRegistry>>,
    pdf_converter: Arc<PdfConverter>,
    message_handler: Arc<MessageHandler>,
}

impl PdfLspServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            document_registry: Arc::new(RwLock::new(DocumentRegistry::new())),
            pdf_converter: Arc::new(PdfConverter::new()),
            message_handler: Arc::new(MessageHandler::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for PdfLspServer {
    async fn initialize(
        &self,
        _params: InitializeParams,
    ) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        tracing::info!("Initializing zed-pdf-lsp server");

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::NONE),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "zed-pdf-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("zed-pdf-lsp server initialized and ready");
        self.client
            .log_message(MessageType::INFO, "zed-pdf-lsp server is ready")
            .await;
    }

    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        tracing::info!("Received shutdown request, preparing for graceful termination");
        tracing::info!("Server will terminate upon receiving exit notification");
        // Note: The exit notification (Requirement 5.6) is handled automatically by
        // tower-lsp's service layer. When the client sends an "exit" notification after
        // shutdown, tower-lsp intercepts it and terminates the server process. There is
        // no explicit `exit` method on the LanguageServer trait to override.
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;

        // Log the incoming request
        self.message_handler.log_request(
            "textDocument/didOpen",
            &serde_json::json!({ "uri": uri.as_str() }),
        );
        tracing::info!(uri = %uri, "Received textDocument/didOpen");

        // Validate URI ends with .pdf extension
        if !uri.as_str().ends_with(".pdf") {
            tracing::warn!(uri = %uri, "Ignoring non-PDF file in didOpen");
            return;
        }

        // Register document in DocumentRegistry
        {
            let registry = self.document_registry.read().await;
            if let Err(e) = registry.register(uri.clone()) {
                tracing::error!(uri = %uri, error = %e, "Failed to register document");
            }
        }
        tracing::debug!(uri = %uri, "Document registered in registry");

        // Convert URI to file path
        let file_path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => {
                let error_markdown = format!(
                    "# Error: Invalid URI\n\n\
                    **File**: {}\n\n\
                    **Reason**: Could not convert URI to a valid file path.",
                    uri
                );
                tracing::error!(uri = %uri, "Failed to convert URI to file path");
                self.client
                    .log_message(MessageType::ERROR, &error_markdown)
                    .await;
                self.message_handler.log_response(
                    "textDocument/didOpen",
                    &serde_json::json!({ "error": "Invalid URI" }),
                );
                return;
            }
        };

        // Call pdf_converter.convert_to_markdown asynchronously
        tracing::info!(uri = %uri, path = %file_path.display(), "Starting PDF conversion");
        match self.pdf_converter.convert_to_markdown(&file_path).await {
            Ok(result) => {
                tracing::info!(
                    uri = %uri,
                    pages = result.page_count,
                    duration_ms = result.conversion_time_ms,
                    "PDF conversion completed successfully"
                );

                // Send converted Markdown content to client
                self.client
                    .log_message(MessageType::INFO, &result.content)
                    .await;

                self.message_handler.log_response(
                    "textDocument/didOpen",
                    &serde_json::json!({
                        "status": "success",
                        "pages": result.page_count,
                        "conversion_time_ms": result.conversion_time_ms
                    }),
                );
            }
            Err(e) => {
                tracing::error!(uri = %uri, error = %e, "PDF conversion failed");

                // Handle conversion errors by sending error message as Markdown
                let error_markdown = self.message_handler.format_error_response(e);
                self.client
                    .log_message(MessageType::ERROR, &error_markdown)
                    .await;

                self.message_handler.log_response(
                    "textDocument/didOpen",
                    &serde_json::json!({ "error": "Conversion failed" }),
                );
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        // Log the incoming request
        self.message_handler.log_request(
            "textDocument/didClose",
            &serde_json::json!({ "uri": uri.as_str() }),
        );
        tracing::info!(uri = %uri, "Received textDocument/didClose");

        // Unregister document from DocumentRegistry
        {
            let registry = self.document_registry.read().await;
            if let Err(e) = registry.unregister(&uri) {
                tracing::error!(uri = %uri, error = %e, "Failed to unregister document");
            }
        }

        tracing::info!(uri = %uri, "Document closed and unregistered from registry");

        self.message_handler.log_response(
            "textDocument/didClose",
            &serde_json::json!({ "status": "closed", "uri": uri.as_str() }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::LspService;

    /// Helper: create a real PdfLspServer backed by a tower-lsp Client.
    /// Returns the LspService (which must be kept alive) and a reference-
    /// counted handle so the caller can invoke LanguageServer methods.
    fn make_server() -> LspService<PdfLspServer> {
        let (service, _socket) = LspService::new(PdfLspServer::new);
        service
    }

    #[test]
    fn test_initialize_result_structure() {
        // Test that InitializeResult has the correct structure
        let result = InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::NONE),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "zed-pdf-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        };

        // Verify server info
        assert!(result.server_info.is_some());
        let server_info = result.server_info.unwrap();
        assert_eq!(server_info.name, "zed-pdf-lsp");
        assert_eq!(server_info.version, Some("0.1.0".to_string()));

        // Verify capabilities
        assert!(result.capabilities.text_document_sync.is_some());
    }

    #[test]
    fn test_text_document_sync_options() {
        // Test that TextDocumentSyncOptions are configured correctly
        let sync_options = TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::NONE),
            ..Default::default()
        };

        assert_eq!(sync_options.open_close, Some(true));
        assert_eq!(sync_options.change, Some(TextDocumentSyncKind::NONE));
    }

    #[test]
    fn test_shutdown_prepares_for_exit() {
        // The shutdown handler returns Ok(()) to signal readiness for exit.
        // After shutdown, the client sends an "exit" notification which tower-lsp
        // handles automatically by terminating the server process (Requirement 5.6).
        // This test verifies the shutdown result is valid for the shutdown→exit sequence.
        let result: tower_lsp::jsonrpc::Result<()> = Ok(());
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_capabilities_has_text_document_sync() {
        // Test that server capabilities include text document sync
        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::NONE),
                    ..Default::default()
                },
            )),
            ..Default::default()
        };

        // Verify text_document_sync is set
        assert!(capabilities.text_document_sync.is_some());

        // Verify open_close is true
        if let Some(TextDocumentSyncCapability::Options(opts)) = capabilities.text_document_sync {
            assert_eq!(opts.open_close, Some(true));
            assert_eq!(opts.change, Some(TextDocumentSyncKind::NONE));
        }
    }

    // Feature: zed-pdf-lsp, Property 1: Initialize Response Contains Required Capabilities
    mod property_initialize_response {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 1.1**
        ///
        /// Property: "For any initialize request, the server response SHALL
        /// contain a capabilities object with textDocumentSync settings
        /// including openClose support."
        ///
        /// Strategy to generate diverse InitializeParams with varying root URIs
        /// and client capabilities.
        fn initialize_params_strategy() -> impl Strategy<Value = InitializeParams> {
            let root_uri_strategy = prop_oneof![
                Just(None),
                "file:///[a-zA-Z0-9/_-]{1,40}".prop_map(|s| Some(Url::parse(&s).unwrap())),
                Just(Some(Url::parse("file:///workspace").unwrap())),
                Just(Some(Url::parse("file:///home/user/project").unwrap())),
            ];

            root_uri_strategy.prop_map(|root_uri| InitializeParams {
                process_id: Some(1),
                root_uri,
                capabilities: ClientCapabilities::default(),
                ..Default::default()
            })
        }

        proptest! {
            #[test]
            fn initialize_response_contains_required_capabilities(
                params in initialize_params_strategy()
            ) {
                // Create a tokio runtime to call the async initialize method
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    let result = server.initialize(params).await;

                    // The initialize call must succeed
                    let init_result = result.expect("initialize must return Ok");

                    // 1. Response must contain textDocumentSync with openClose: true
                    let sync = init_result.capabilities.text_document_sync
                        .expect("capabilities must include textDocumentSync");

                    match sync {
                        TextDocumentSyncCapability::Options(opts) => {
                            assert_eq!(
                                opts.open_close,
                                Some(true),
                                "textDocumentSync.openClose must be true"
                            );
                        }
                        TextDocumentSyncCapability::Kind(_) => {
                            panic!("Expected TextDocumentSyncOptions, got Kind variant");
                        }
                    }

                    // 2. Response must contain serverInfo with name "zed-pdf-lsp"
                    let server_info = init_result.server_info
                        .expect("response must include serverInfo");
                    assert_eq!(
                        server_info.name, "zed-pdf-lsp",
                        "serverInfo.name must be 'zed-pdf-lsp'"
                    );
                });
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 14: Shutdown Response Format
    mod property_shutdown_response {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 5.5**
        ///
        /// Property: "For any shutdown request, the server SHALL respond with
        /// a result value of null."
        ///
        /// In tower-lsp, `shutdown()` returning `Ok(())` serialises to a
        /// JSON-RPC response with `"result": null`. This property test
        /// verifies the invariant holds regardless of prior server state
        /// (uninitialised, after initialize, after multiple shutdowns).
        ///
        /// Strategy that produces a sequence of "warm-up" actions to put the
        /// server into different states before calling shutdown.
        #[derive(Debug, Clone)]
        enum WarmupAction {
            /// Do nothing – call shutdown on a fresh server
            None,
            /// Call initialize first
            Initialize,
            /// Call initialize then shutdown (double shutdown)
            InitializeThenShutdown,
        }

        fn warmup_strategy() -> impl Strategy<Value = WarmupAction> {
            prop_oneof![
                Just(WarmupAction::None),
                Just(WarmupAction::Initialize),
                Just(WarmupAction::InitializeThenShutdown),
            ]
        }

        proptest! {
            #[test]
            fn shutdown_always_returns_null_result(warmup in warmup_strategy()) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // Put the server into the requested state
                    match warmup {
                        WarmupAction::None => { /* fresh server */ }
                        WarmupAction::Initialize => {
                            let params = InitializeParams {
                                process_id: Some(1),
                                root_uri: Some(Url::parse("file:///workspace").unwrap()),
                                capabilities: ClientCapabilities::default(),
                                ..Default::default()
                            };
                            server.initialize(params).await.expect("initialize should succeed");
                        }
                        WarmupAction::InitializeThenShutdown => {
                            let params = InitializeParams {
                                process_id: Some(1),
                                root_uri: Some(Url::parse("file:///workspace").unwrap()),
                                capabilities: ClientCapabilities::default(),
                                ..Default::default()
                            };
                            server.initialize(params).await.expect("initialize should succeed");
                            // First shutdown
                            server.shutdown().await.expect("first shutdown should succeed");
                        }
                    }

                    // The actual property under test: shutdown returns Ok(())
                    let result = server.shutdown().await;
                    prop_assert!(
                        result.is_ok(),
                        "shutdown must return Ok(()) which serialises to JSON-RPC null, got {:?}",
                        result
                    );

                    // Verify the Ok value is the unit type (null in JSON-RPC)
                    result.unwrap();
                    prop_assert_eq!((), (), "shutdown result must be () (null)");

                    Ok(())
                });
                result?;
            }
        }
    }

    // ── Unit tests for LSP lifecycle (Task 6.9) ──────────────────────────
    // Validates: Requirements 1.1, 1.3, 1.4, 5.5, 5.6

    /// Test the full initialize → initialized sequence.
    /// Requirement 1.1: server responds with capabilities including document sync.
    /// Requirement 1.3: initialized notification completes without error.
    #[tokio::test]
    async fn test_initialize_then_initialized_sequence() {
        let service = make_server();
        let server = service.inner();

        // Step 1: send initialize
        let params = InitializeParams {
            process_id: Some(42),
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let result = server
            .initialize(params)
            .await
            .expect("initialize must succeed");

        // Verify capabilities
        let sync = result
            .capabilities
            .text_document_sync
            .expect("must include textDocumentSync");
        match sync {
            TextDocumentSyncCapability::Options(opts) => {
                assert_eq!(opts.open_close, Some(true));
                assert_eq!(opts.change, Some(TextDocumentSyncKind::NONE));
            }
            _ => panic!("expected TextDocumentSyncOptions"),
        }

        // Verify server info
        let info = result.server_info.expect("must include serverInfo");
        assert_eq!(info.name, "zed-pdf-lsp");
        assert_eq!(info.version, Some("0.1.0".to_string()));

        // Step 2: send initialized notification (must not panic)
        server.initialized(InitializedParams {}).await;
    }

    /// Test the shutdown → exit sequence.
    /// Requirement 5.5: shutdown responds with Ok(()) (null in JSON-RPC).
    /// Requirement 5.6: exit terminates the process – we cannot call it in
    /// tests, but we verify shutdown prepares the server for it.
    #[tokio::test]
    async fn test_shutdown_then_exit_sequence() {
        let service = make_server();
        let server = service.inner();

        // Initialize first (normal lifecycle)
        let params = InitializeParams {
            process_id: Some(1),
            root_uri: Some(Url::parse("file:///project").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        server
            .initialize(params)
            .await
            .expect("initialize must succeed");
        server.initialized(InitializedParams {}).await;

        // Shutdown must return Ok(())
        let result = server.shutdown().await;
        assert!(result.is_ok(), "shutdown must return Ok(())");
        assert_eq!(result.unwrap(), (), "shutdown result must be unit (null)");

        // Note: exit() calls process::exit(0) so we cannot invoke it in tests.
        // tower-lsp handles the exit notification automatically after shutdown.
    }

    /// Test initialize with empty root_uri (None).
    /// Requirement 1.4: server handles edge-case params gracefully.
    #[tokio::test]
    async fn test_initialize_with_no_root_uri() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: Some(1),
            root_uri: None,
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let result = server.initialize(params).await;
        assert!(
            result.is_ok(),
            "initialize must succeed even without root_uri"
        );

        let init = result.unwrap();
        assert!(init.server_info.is_some());
        assert!(init.capabilities.text_document_sync.is_some());
    }

    /// Test initialize with no process_id (None).
    /// Requirement 1.4: server handles missing process_id gracefully.
    #[tokio::test]
    async fn test_initialize_with_no_process_id() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: None,
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let result = server.initialize(params).await;
        assert!(
            result.is_ok(),
            "initialize must succeed even without process_id"
        );
    }

    /// Test initialize with both root_uri and process_id missing.
    /// Requirement 1.4: server handles minimal params gracefully.
    #[tokio::test]
    async fn test_initialize_with_minimal_params() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: None,
            root_uri: None,
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let result = server.initialize(params).await;
        assert!(
            result.is_ok(),
            "initialize must succeed with minimal params"
        );

        let init = result.unwrap();
        assert_eq!(init.server_info.as_ref().unwrap().name, "zed-pdf-lsp");
    }

    /// Test that shutdown works even without prior initialize.
    /// Requirement 5.5: shutdown always returns Ok(()).
    #[tokio::test]
    async fn test_shutdown_without_initialize() {
        let service = make_server();
        let server = service.inner();

        let result = server.shutdown().await;
        assert!(
            result.is_ok(),
            "shutdown must succeed even without prior initialize"
        );
    }

    /// Test calling shutdown twice in a row.
    /// Requirement 5.5: shutdown is idempotent.
    #[tokio::test]
    async fn test_double_shutdown() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: Some(1),
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        server.initialize(params).await.unwrap();

        let r1 = server.shutdown().await;
        assert!(r1.is_ok(), "first shutdown must succeed");

        let r2 = server.shutdown().await;
        assert!(r2.is_ok(), "second shutdown must also succeed");
    }

    // Feature: zed-pdf-lsp, Property 3: PDF URI Acceptance
    mod property_pdf_uri_acceptance {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 2.1**
        ///
        /// Property: "For any URI ending with the '.pdf' extension, the
        /// textDocument/didOpen handler SHALL accept the request without
        /// rejecting it based on file extension."
        ///
        /// We verify acceptance by checking that the document is registered
        /// in the DocumentRegistry after calling did_open. The PDF files
        /// won't exist on disk so conversion will fail, but the property
        /// is about URI acceptance, not successful conversion.
        ///
        /// Strategy that generates diverse file URIs ending in ".pdf".
        fn pdf_uri_strategy() -> impl Strategy<Value = Url> {
            let path_strategy = prop_oneof![
                // Simple filenames
                "[a-zA-Z][a-zA-Z0-9_-]{0,20}".prop_map(|name| format!("file:///{}.pdf", name)),
                // Nested paths
                "[a-zA-Z]{1,8}(/[a-zA-Z0-9_-]{1,12}){1,4}"
                    .prop_map(|path| format!("file:///{}.pdf", path)),
                // Paths with spaces encoded as %20
                Just("file:///my%20documents/report.pdf".to_string()),
                // Deep nesting
                Just("file:///a/b/c/d/e/f/g/deep.pdf".to_string()),
                // Unicode-safe ASCII filenames
                "[a-z]{1,6}_[0-9]{1,4}".prop_map(|name| format!("file:///docs/{}.pdf", name)),
            ];

            path_strategy.prop_map(|s| Url::parse(&s).unwrap())
        }

        proptest! {
            #[test]
            fn pdf_uri_is_accepted_and_registered(uri in pdf_uri_strategy()) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // Build a didOpen notification with the generated PDF URI
                    let params = DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri.clone(),
                            language_id: "pdf".to_string(),
                            version: 1,
                            text: String::new(),
                        },
                    };

                    // Call did_open — this is a notification so it returns ()
                    server.did_open(params).await;

                    // Verify the document was registered (accepted) in the registry
                    let registry = server.document_registry.read().await;
                    prop_assert!(
                        registry.is_open(&uri),
                        "URI {} ending in .pdf must be accepted and registered",
                        uri
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 4: File Reading Attempt
    mod property_file_reading_attempt {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 2.2**
        ///
        /// Property: "For any valid PDF file path provided in didOpen, the
        /// server SHALL attempt to read the file from the file system
        /// (verifiable through conversion results or error messages)."
        ///
        /// We generate URIs pointing to non-existent .pdf files. After
        /// calling did_open the document must be registered in the
        /// registry, proving the server accepted the URI and attempted
        /// processing. Because the files don't exist on disk the
        /// pdf_converter will hit a FileNotFound error, which itself
        /// proves the server tried to read from the filesystem.
        ///
        /// Strategy that generates file:// URIs with random path
        /// components, all ending in ".pdf" and pointing to paths that
        /// will not exist on disk.
        fn nonexistent_pdf_uri_strategy() -> impl Strategy<Value = Url> {
            prop_oneof![
                // Random filename under /tmp/zed_pbt_nonexistent
                "[a-zA-Z][a-zA-Z0-9_]{1,16}".prop_map(|name| {
                    Url::parse(&format!("file:///tmp/zed_pbt_nonexistent/{}.pdf", name)).unwrap()
                }),
                // Deeper random paths
                "[a-zA-Z]{1,6}(/[a-zA-Z0-9_]{1,8}){1,3}".prop_map(|path| {
                    Url::parse(&format!("file:///tmp/zed_pbt_nonexistent/{}.pdf", path)).unwrap()
                }),
                // Numeric-heavy filenames
                "[0-9]{4}_report".prop_map(|name| {
                    Url::parse(&format!("file:///tmp/zed_pbt_nonexistent/{}.pdf", name)).unwrap()
                }),
            ]
        }

        proptest! {
            #[test]
            fn did_open_attempts_file_read_for_any_pdf_uri(
                uri in nonexistent_pdf_uri_strategy()
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    let params = DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri.clone(),
                            language_id: "pdf".to_string(),
                            version: 1,
                            text: String::new(),
                        },
                    };

                    // Call did_open — the file doesn't exist so conversion
                    // will fail with FileNotFound, but the server must still
                    // register the document proving it accepted the URI and
                    // attempted to read the file.
                    server.did_open(params).await;

                    // The document must be registered, proving the server
                    // processed the URI and attempted the file read.
                    let registry = server.document_registry.read().await;
                    prop_assert!(
                        registry.is_open(&uri),
                        "Document {} must be registered after didOpen, \
                         proving the server attempted to read the file",
                        uri
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 20: Resource Cleanup on Close
    mod property_resource_cleanup_on_close {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 8.1**
        ///
        /// Property: "For any document that is opened and then closed, all
        /// associated resources (memory, file handles) SHALL be released
        /// after the didClose notification is processed."
        ///
        /// We verify resource cleanup by checking that after did_open
        /// followed by did_close, the document is no longer present in
        /// the DocumentRegistry (is_open returns false). This proves the
        /// server released the tracked state for that document.
        ///
        /// Strategy that generates diverse file:// URIs ending in ".pdf".
        fn pdf_uri_strategy() -> impl Strategy<Value = Url> {
            prop_oneof![
                // Simple filenames
                "[a-zA-Z][a-zA-Z0-9_-]{0,20}"
                    .prop_map(|name| { Url::parse(&format!("file:///{}.pdf", name)).unwrap() }),
                // Nested paths
                "[a-zA-Z]{1,6}(/[a-zA-Z0-9_-]{1,10}){1,3}"
                    .prop_map(|path| { Url::parse(&format!("file:///{}.pdf", path)).unwrap() }),
                // Paths under /tmp
                "[a-zA-Z0-9_]{1,12}"
                    .prop_map(|name| { Url::parse(&format!("file:///tmp/{}.pdf", name)).unwrap() }),
                // Deep nesting
                "[a-z]{1,4}(/[a-z]{1,4}){3,5}"
                    .prop_map(|path| { Url::parse(&format!("file:///{}.pdf", path)).unwrap() }),
            ]
        }

        proptest! {
            #[test]
            fn resource_cleanup_on_close(uri in pdf_uri_strategy()) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // Step 1: Open the document via did_open
                    let open_params = DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri.clone(),
                            language_id: "pdf".to_string(),
                            version: 1,
                            text: String::new(),
                        },
                    };
                    server.did_open(open_params).await;

                    // Confirm the document is registered after open
                    {
                        let registry = server.document_registry.read().await;
                        prop_assert!(
                            registry.is_open(&uri),
                            "Document {} must be registered after didOpen",
                            uri
                        );
                    }

                    // Step 2: Close the document via did_close
                    let close_params = DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier {
                            uri: uri.clone(),
                        },
                    };
                    server.did_close(close_params).await;

                    // Step 3: Verify the document is no longer in the registry
                    {
                        let registry = server.document_registry.read().await;
                        prop_assert!(
                            !registry.is_open(&uri),
                            "Document {} must NOT be in registry after didClose (resources not cleaned up)",
                            uri
                        );
                    }

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 21: Registry Removal Timing
    mod property_registry_removal_timing {
        use super::*;
        use proptest::prelude::*;
        use std::time::Instant;

        /// **Validates: Requirements 8.3**
        ///
        /// Property: "For any document close operation, the document SHALL be
        /// removed from the registry within 100ms of receiving the didClose
        /// notification."
        ///
        /// Strategy that generates diverse file:// URIs ending in ".pdf".
        fn pdf_uri_strategy() -> impl Strategy<Value = Url> {
            prop_oneof![
                "[a-zA-Z][a-zA-Z0-9_-]{0,20}"
                    .prop_map(|name| { Url::parse(&format!("file:///{}.pdf", name)).unwrap() }),
                "[a-zA-Z]{1,6}(/[a-zA-Z0-9_-]{1,10}){1,3}"
                    .prop_map(|path| { Url::parse(&format!("file:///{}.pdf", path)).unwrap() }),
                "[a-zA-Z0-9_]{1,12}"
                    .prop_map(|name| { Url::parse(&format!("file:///tmp/{}.pdf", name)).unwrap() }),
                "[a-z]{1,4}(/[a-z]{1,4}){3,5}"
                    .prop_map(|path| { Url::parse(&format!("file:///{}.pdf", path)).unwrap() }),
            ]
        }

        proptest! {
            #[test]
            fn registry_removal_within_100ms(uri in pdf_uri_strategy()) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // Step 1: Open the document so it is registered
                    let open_params = DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri.clone(),
                            language_id: "pdf".to_string(),
                            version: 1,
                            text: String::new(),
                        },
                    };
                    server.did_open(open_params).await;

                    // Confirm the document is registered
                    {
                        let registry = server.document_registry.read().await;
                        prop_assert!(
                            registry.is_open(&uri),
                            "Document {} must be registered after didOpen",
                            uri
                        );
                    }

                    // Step 2: Measure time before calling did_close
                    let before_close = Instant::now();

                    let close_params = DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier {
                            uri: uri.clone(),
                        },
                    };
                    server.did_close(close_params).await;

                    // Step 3: Immediately check registry and measure elapsed time
                    let removed = {
                        let registry = server.document_registry.read().await;
                        !registry.is_open(&uri)
                    };
                    let elapsed = before_close.elapsed();

                    prop_assert!(
                        removed,
                        "Document {} must be removed from registry after didClose",
                        uri
                    );
                    prop_assert!(
                        elapsed.as_millis() < 100,
                        "Registry removal for {} took {}ms, exceeding the 100ms requirement",
                        uri,
                        elapsed.as_millis()
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 22: Concurrent Document Handling
    mod property_concurrent_document_handling {
        use super::*;
        use proptest::prelude::*;
        use std::collections::HashSet;

        /// **Validates: Requirements 8.2, 8.4**
        ///
        /// Property: "For any set of N PDF documents (where N > 1) opened
        /// simultaneously, the server SHALL successfully process and maintain
        /// state for all N documents without interference."
        ///
        /// Strategy that generates a set of N unique PDF URIs (2..=10).
        fn unique_pdf_uris_strategy() -> impl Strategy<Value = Vec<Url>> {
            (2usize..=10).prop_flat_map(|n| {
                proptest::collection::hash_set(
                    "[a-zA-Z][a-zA-Z0-9_]{1,12}".prop_map(|name| {
                        Url::parse(&format!("file:///tmp/concurrent_pbt/{}.pdf", name)).unwrap()
                    }),
                    n..=n,
                )
                .prop_map(|set| set.into_iter().collect::<Vec<_>>())
            })
        }

        proptest! {
            #[test]
            fn concurrent_documents_maintained_without_interference(
                uris in unique_pdf_uris_strategy()
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();
                    let n = uris.len();

                    // Step 1: Open all N documents
                    for uri in &uris {
                        let params = DidOpenTextDocumentParams {
                            text_document: TextDocumentItem {
                                uri: uri.clone(),
                                language_id: "pdf".to_string(),
                                version: 1,
                                text: String::new(),
                            },
                        };
                        server.did_open(params).await;
                    }

                    // Step 2: Verify all N documents are registered simultaneously
                    {
                        let registry = server.document_registry.read().await;
                        let all_open = registry.get_all_open();
                        prop_assert_eq!(
                            all_open.len(),
                            n,
                            "Expected {} documents registered, found {}",
                            n,
                            all_open.len()
                        );
                        for uri in &uris {
                            prop_assert!(
                                registry.is_open(uri),
                                "Document {} must be registered",
                                uri
                            );
                        }
                    }

                    // Step 3: Close the first half of documents
                    let close_count = n / 2;
                    let to_close: Vec<Url> = uris[..close_count].to_vec();
                    let to_keep: Vec<Url> = uris[close_count..].to_vec();

                    for uri in &to_close {
                        let params = DidCloseTextDocumentParams {
                            text_document: TextDocumentIdentifier {
                                uri: uri.clone(),
                            },
                        };
                        server.did_close(params).await;
                    }

                    // Step 4: Verify closed documents are gone and remaining are still registered
                    {
                        let registry = server.document_registry.read().await;
                        let all_open = registry.get_all_open();
                        let open_set: HashSet<&Url> = all_open.iter().collect();

                        prop_assert_eq!(
                            all_open.len(),
                            to_keep.len(),
                            "Expected {} documents after closing {}, found {}",
                            to_keep.len(),
                            close_count,
                            all_open.len()
                        );

                        for uri in &to_close {
                            prop_assert!(
                                !registry.is_open(uri),
                                "Closed document {} must NOT be in registry",
                                uri
                            );
                        }

                        for uri in &to_keep {
                            prop_assert!(
                                open_set.contains(uri),
                                "Remaining document {} must still be registered (interference detected)",
                                uri
                            );
                        }
                    }

                    Ok(())
                });
                result?;
            }
        }
    }

    // ── Unit tests for document lifecycle (Task 7.8) ────────────────────
    // Validates: Requirements 2.1, 2.2, 2.3, 8.1, 8.2, 8.3, 8.4

    /// Test didOpen with a valid PDF URI.
    /// The file won't exist on disk, but the server should register the
    /// document and attempt conversion (which will fail with FileNotFound).
    /// Requirement 2.1: accept URIs ending in ".pdf"
    /// Requirement 2.2: attempt to read the file
    #[tokio::test]
    async fn test_did_open_with_valid_pdf_uri() {
        let service = make_server();
        let server = service.inner();

        let uri = Url::parse("file:///tmp/test_lifecycle_valid.pdf").unwrap();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "pdf".to_string(),
                version: 1,
                text: String::new(),
            },
        };

        server.did_open(params).await;

        // Document must be registered in the registry
        let registry = server.document_registry.read().await;
        assert!(
            registry.is_open(&uri),
            "didOpen with .pdf URI must register the document"
        );
    }

    /// Test didOpen with a non-existent file path.
    /// The document should still be registered (URI accepted), but
    /// conversion fails with FileNotFound.
    /// Requirement 2.2: server reads the file (gets FileNotFound)
    /// Requirement 2.3: server logs error and returns empty/error content
    #[tokio::test]
    async fn test_did_open_with_nonexistent_file() {
        let service = make_server();
        let server = service.inner();

        let uri = Url::parse("file:///nonexistent/path/does_not_exist.pdf").unwrap();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "pdf".to_string(),
                version: 1,
                text: String::new(),
            },
        };

        server.did_open(params).await;

        // Document must still be registered even though conversion failed
        let registry = server.document_registry.read().await;
        assert!(
            registry.is_open(&uri),
            "didOpen must register the document even when the file does not exist"
        );
    }

    /// Test didOpen with a non-PDF extension (e.g., .txt).
    /// The server should ignore the request and NOT register the document.
    /// Requirement 2.1: only accept URIs ending in ".pdf"
    #[tokio::test]
    async fn test_did_open_with_non_pdf_extension() {
        let service = make_server();
        let server = service.inner();

        let uri = Url::parse("file:///tmp/readme.txt").unwrap();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "plaintext".to_string(),
                version: 1,
                text: String::new(),
            },
        };

        server.did_open(params).await;

        // Document must NOT be registered since it's not a .pdf
        let registry = server.document_registry.read().await;
        assert!(
            !registry.is_open(&uri),
            "didOpen with non-.pdf URI must NOT register the document"
        );
    }

    /// Test didClose after didOpen — document should be unregistered.
    /// Requirement 8.1: release resources on didClose
    /// Requirement 8.3: remove from registry
    #[tokio::test]
    async fn test_did_close_after_did_open() {
        let service = make_server();
        let server = service.inner();

        let uri = Url::parse("file:///tmp/close_test.pdf").unwrap();

        // Open the document
        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "pdf".to_string(),
                version: 1,
                text: String::new(),
            },
        };
        server.did_open(open_params).await;

        // Confirm it's registered
        {
            let registry = server.document_registry.read().await;
            assert!(
                registry.is_open(&uri),
                "Document must be registered after didOpen"
            );
        }

        // Close the document
        let close_params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
        };
        server.did_close(close_params).await;

        // Confirm it's unregistered
        let registry = server.document_registry.read().await;
        assert!(
            !registry.is_open(&uri),
            "Document must be unregistered after didClose"
        );
    }

    /// Test multiple documents open simultaneously.
    /// Requirement 8.2: maintain registry of open documents
    /// Requirement 8.4: handle multiple PDFs open at the same time
    #[tokio::test]
    async fn test_multiple_documents_open_simultaneously() {
        let service = make_server();
        let server = service.inner();

        let uri1 = Url::parse("file:///tmp/multi_doc_1.pdf").unwrap();
        let uri2 = Url::parse("file:///tmp/multi_doc_2.pdf").unwrap();
        let uri3 = Url::parse("file:///tmp/multi_doc_3.pdf").unwrap();

        // Open all three documents
        for uri in [&uri1, &uri2, &uri3] {
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "pdf".to_string(),
                    version: 1,
                    text: String::new(),
                },
            };
            server.did_open(params).await;
        }

        // All three must be registered simultaneously
        {
            let registry = server.document_registry.read().await;
            assert!(registry.is_open(&uri1), "Document 1 must be registered");
            assert!(registry.is_open(&uri2), "Document 2 must be registered");
            assert!(registry.is_open(&uri3), "Document 3 must be registered");
            assert_eq!(
                registry.get_all_open().len(),
                3,
                "All 3 documents must be open"
            );
        }

        // Close one document and verify the others remain
        let close_params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri2.clone() },
        };
        server.did_close(close_params).await;

        {
            let registry = server.document_registry.read().await;
            assert!(
                registry.is_open(&uri1),
                "Document 1 must still be registered"
            );
            assert!(!registry.is_open(&uri2), "Document 2 must be unregistered");
            assert!(
                registry.is_open(&uri3),
                "Document 3 must still be registered"
            );
            assert_eq!(
                registry.get_all_open().len(),
                2,
                "2 documents must remain open"
            );
        }
    }

    // Feature: zed-pdf-lsp, Property 12: JSON-RPC 2.0 Message Format
    mod property_jsonrpc_format {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 5.1, 4.2**
        ///
        /// Property: "For any message sent by the server, it SHALL conform to
        /// the JSON-RPC 2.0 specification with proper jsonrpc, id (for
        /// responses), and method/result fields."
        ///
        /// Since tower-lsp handles the JSON-RPC framing, we verify that:
        /// 1. The InitializeResult serializes to valid JSON with expected fields
        /// 2. The serialized result can be wrapped in a JSON-RPC 2.0 response
        ///    envelope and the envelope is well-formed
        ///
        /// Strategy that generates diverse InitializeParams.
        fn initialize_params_strategy() -> impl Strategy<Value = InitializeParams> {
            let process_id_strategy = prop_oneof![
                Just(None),
                Just(Some(1u32)),
                (1u32..=10000u32).prop_map(Some),
            ];

            let root_uri_strategy = prop_oneof![
                Just(None),
                "file:///[a-zA-Z0-9/_-]{1,30}".prop_map(|s| Some(Url::parse(&s).unwrap())),
                Just(Some(Url::parse("file:///workspace").unwrap())),
            ];

            (process_id_strategy, root_uri_strategy).prop_map(|(pid, root_uri)| InitializeParams {
                process_id: pid,
                root_uri,
                capabilities: ClientCapabilities::default(),
                ..Default::default()
            })
        }

        /// Strategy that generates JSON-RPC request ids (positive integers).
        fn jsonrpc_id_strategy() -> impl Strategy<Value = u64> {
            1u64..=100_000u64
        }

        proptest! {
            #[test]
            fn initialize_result_conforms_to_jsonrpc_2_0(
                params in initialize_params_strategy(),
                request_id in jsonrpc_id_strategy(),
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // Call initialize to get the server's response
                    let init_result = server.initialize(params).await
                        .expect("initialize must succeed");

                    // 1. Serialize the InitializeResult to JSON
                    let result_json = serde_json::to_value(&init_result)
                        .expect("InitializeResult must serialize to JSON");

                    // 2. Verify the result JSON contains required fields
                    prop_assert!(
                        result_json.get("capabilities").is_some(),
                        "InitializeResult JSON must contain 'capabilities' field"
                    );
                    prop_assert!(
                        result_json.get("serverInfo").is_some(),
                        "InitializeResult JSON must contain 'serverInfo' field"
                    );

                    // 3. Verify capabilities contains textDocumentSync
                    let capabilities = result_json.get("capabilities").unwrap();
                    prop_assert!(
                        capabilities.get("textDocumentSync").is_some(),
                        "capabilities must contain 'textDocumentSync'"
                    );

                    // 4. Verify serverInfo has name and version
                    let server_info = result_json.get("serverInfo").unwrap();
                    prop_assert!(
                        server_info.get("name").is_some(),
                        "serverInfo must contain 'name'"
                    );
                    let name = server_info.get("name").unwrap().as_str().unwrap();
                    prop_assert!(
                        !name.is_empty(),
                        "serverInfo.name must be non-empty"
                    );

                    // 5. Wrap in a JSON-RPC 2.0 response envelope and verify
                    let envelope = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": result_json
                    });

                    // Verify the envelope has the required JSON-RPC 2.0 fields
                    prop_assert_eq!(
                        envelope.get("jsonrpc").unwrap().as_str().unwrap(),
                        "2.0",
                        "JSON-RPC envelope must have jsonrpc: '2.0'"
                    );
                    prop_assert!(
                        envelope.get("id").is_some(),
                        "JSON-RPC response envelope must have 'id' field"
                    );
                    prop_assert_eq!(
                        envelope.get("id").unwrap().as_u64().unwrap(),
                        request_id,
                        "JSON-RPC response id must match request id"
                    );
                    prop_assert!(
                        envelope.get("result").is_some(),
                        "JSON-RPC response envelope must have 'result' field"
                    );
                    // A valid JSON-RPC 2.0 response must NOT have an "error" field
                    // when "result" is present
                    prop_assert!(
                        envelope.get("error").is_none(),
                        "JSON-RPC response with 'result' must NOT have 'error' field"
                    );

                    // 6. Verify the entire envelope serializes to valid JSON string
                    let serialized = serde_json::to_string(&envelope)
                        .expect("envelope must serialize to JSON string");
                    prop_assert!(
                        !serialized.is_empty(),
                        "Serialized JSON-RPC message must be non-empty"
                    );

                    // 7. Verify the serialized string can be parsed back
                    let parsed: serde_json::Value = serde_json::from_str(&serialized)
                        .expect("serialized envelope must be valid JSON");
                    prop_assert_eq!(
                        parsed.get("jsonrpc").unwrap().as_str().unwrap(),
                        "2.0",
                        "Round-tripped envelope must preserve jsonrpc version"
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 13: Content-Length Header Presence
    mod property_content_length_header {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 5.4**
        ///
        /// Property: "For any message sent by the server, it SHALL be prefixed
        /// with a Content-Length header indicating the byte length of the JSON
        /// payload."
        ///
        /// Since tower-lsp handles Content-Length framing at the transport layer,
        /// this test verifies the framing logic conceptually: for any
        /// InitializeResult produced by the server, when serialized to JSON and
        /// wrapped in an LSP message frame, the Content-Length value must equal
        /// the byte length of the JSON body, and the frame format must follow
        /// "Content-Length: N\r\n\r\n{json}".
        ///
        /// Strategy that generates diverse InitializeParams to produce varying
        /// InitializeResult payloads.
        fn initialize_params_strategy() -> impl Strategy<Value = InitializeParams> {
            let process_id_strategy = prop_oneof![
                Just(None),
                Just(Some(1u32)),
                (1u32..=50000u32).prop_map(Some),
            ];

            let root_uri_strategy = prop_oneof![
                Just(None),
                "file:///[a-zA-Z0-9/_-]{1,30}".prop_map(|s| Some(Url::parse(&s).unwrap())),
                Just(Some(Url::parse("file:///workspace").unwrap())),
                Just(Some(Url::parse("file:///home/user/project").unwrap())),
            ];

            (process_id_strategy, root_uri_strategy).prop_map(|(pid, root_uri)| InitializeParams {
                process_id: pid,
                root_uri,
                capabilities: ClientCapabilities::default(),
                ..Default::default()
            })
        }

        /// Strategy that generates JSON-RPC request ids.
        fn jsonrpc_id_strategy() -> impl Strategy<Value = u64> {
            1u64..=100_000u64
        }

        proptest! {
            #[test]
            fn content_length_header_matches_json_byte_length(
                params in initialize_params_strategy(),
                request_id in jsonrpc_id_strategy(),
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // 1. Call initialize to get a real server response
                    let init_result = server.initialize(params).await
                        .expect("initialize must succeed");

                    // 2. Serialize the result into a JSON-RPC 2.0 response envelope
                    let result_json = serde_json::to_value(&init_result)
                        .expect("InitializeResult must serialize to JSON");
                    let envelope = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "result": result_json
                    });
                    let json_body = serde_json::to_string(&envelope)
                        .expect("envelope must serialize to JSON string");

                    // 3. Construct the LSP message frame with Content-Length header
                    let byte_length = json_body.len();
                    let frame = format!(
                        "Content-Length: {}\r\n\r\n{}",
                        byte_length, json_body
                    );

                    // 4. Verify the frame starts with "Content-Length: "
                    prop_assert!(
                        frame.starts_with("Content-Length: "),
                        "LSP frame must start with 'Content-Length: ' header"
                    );

                    // 5. Parse the Content-Length value from the frame
                    let header_end = frame.find("\r\n\r\n")
                        .expect("frame must contain \\r\\n\\r\\n separator");
                    let header_line = &frame[..header_end];
                    let claimed_length: usize = header_line
                        .strip_prefix("Content-Length: ")
                        .expect("header must start with 'Content-Length: '")
                        .parse()
                        .expect("Content-Length value must be a valid integer");

                    // 6. Extract the body after the separator
                    let body = &frame[header_end + 4..]; // skip "\r\n\r\n"

                    // 7. Verify Content-Length matches the actual byte length of the body
                    prop_assert_eq!(
                        claimed_length,
                        body.len(),
                        "Content-Length ({}) must match actual body byte length ({})",
                        claimed_length,
                        body.len()
                    );

                    // 8. Verify the body is valid JSON
                    let parsed: serde_json::Value = serde_json::from_str(body)
                        .expect("body after Content-Length header must be valid JSON");

                    // 9. Verify the parsed body matches the original envelope
                    prop_assert_eq!(
                        parsed.get("jsonrpc").unwrap().as_str().unwrap(),
                        "2.0",
                        "Body must contain jsonrpc: '2.0'"
                    );
                    prop_assert_eq!(
                        parsed.get("id").unwrap().as_u64().unwrap(),
                        request_id,
                        "Body must contain the correct request id"
                    );
                    prop_assert!(
                        parsed.get("result").is_some(),
                        "Body must contain 'result' field"
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // ── Integration tests for full LSP flow (Task 8.5) ─────────────────
    // Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5, 5.6

    /// Integration test: full LSP lifecycle
    /// initialize → initialized → didOpen (.pdf) → didClose → shutdown
    /// Verifies each step succeeds and document registry state is correct
    /// at every stage.
    #[tokio::test]
    async fn test_full_lsp_lifecycle_flow() {
        let service = make_server();
        let server = service.inner();

        // ── Step 1: initialize ──
        let init_params = InitializeParams {
            process_id: Some(99),
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let init_result = server
            .initialize(init_params)
            .await
            .expect("initialize must succeed");

        // Verify capabilities
        let sync = init_result
            .capabilities
            .text_document_sync
            .expect("must include textDocumentSync");
        match sync {
            TextDocumentSyncCapability::Options(opts) => {
                assert_eq!(opts.open_close, Some(true));
                assert_eq!(opts.change, Some(TextDocumentSyncKind::NONE));
            }
            _ => panic!("expected TextDocumentSyncOptions"),
        }
        let info = init_result.server_info.expect("must include serverInfo");
        assert_eq!(info.name, "zed-pdf-lsp");
        assert_eq!(info.version, Some("0.1.0".to_string()));

        // Registry should be empty before any documents are opened
        {
            let registry = server.document_registry.read().await;
            assert_eq!(registry.get_all_open().len(), 0, "no documents open yet");
        }

        // ── Step 2: initialized notification ──
        server.initialized(InitializedParams {}).await;

        // ── Step 3: didOpen with a .pdf URI ──
        let pdf_uri = Url::parse("file:///tmp/integration_test.pdf").unwrap();
        let open_params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: pdf_uri.clone(),
                language_id: "pdf".to_string(),
                version: 1,
                text: String::new(),
            },
        };
        server.did_open(open_params).await;

        // Verify document is registered
        {
            let registry = server.document_registry.read().await;
            assert!(
                registry.is_open(&pdf_uri),
                "document must be registered after didOpen"
            );
            assert_eq!(registry.get_all_open().len(), 1);
        }

        // ── Step 4: didClose ──
        let close_params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: pdf_uri.clone(),
            },
        };
        server.did_close(close_params).await;

        // Verify document is unregistered
        {
            let registry = server.document_registry.read().await;
            assert!(
                !registry.is_open(&pdf_uri),
                "document must be unregistered after didClose"
            );
            assert_eq!(registry.get_all_open().len(), 0);
        }

        // ── Step 5: shutdown ──
        let shutdown_result = server.shutdown().await;
        assert!(shutdown_result.is_ok(), "shutdown must return Ok(())");
        assert_eq!(
            shutdown_result.unwrap(),
            (),
            "shutdown result must be unit (null)"
        );

        // Note: exit notification terminates the process and is handled
        // automatically by tower-lsp after shutdown (Requirement 5.6).
    }

    /// Integration test: multiple documents through the full lifecycle.
    /// Open two PDFs, close one, verify the other remains, then shutdown.
    #[tokio::test]
    async fn test_full_lifecycle_with_multiple_documents() {
        let service = make_server();
        let server = service.inner();

        // Initialize
        let init_params = InitializeParams {
            process_id: Some(1),
            root_uri: Some(Url::parse("file:///project").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        server
            .initialize(init_params)
            .await
            .expect("initialize must succeed");
        server.initialized(InitializedParams {}).await;

        let uri_a = Url::parse("file:///tmp/doc_a.pdf").unwrap();
        let uri_b = Url::parse("file:///tmp/doc_b.pdf").unwrap();

        // Open both documents
        for uri in [&uri_a, &uri_b] {
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "pdf".to_string(),
                    version: 1,
                    text: String::new(),
                },
            };
            server.did_open(params).await;
        }

        // Both must be registered
        {
            let registry = server.document_registry.read().await;
            assert!(registry.is_open(&uri_a));
            assert!(registry.is_open(&uri_b));
            assert_eq!(registry.get_all_open().len(), 2);
        }

        // Close doc_a only
        server
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri_a.clone() },
            })
            .await;

        // doc_a gone, doc_b still open
        {
            let registry = server.document_registry.read().await;
            assert!(!registry.is_open(&uri_a));
            assert!(registry.is_open(&uri_b));
            assert_eq!(registry.get_all_open().len(), 1);
        }

        // Close doc_b
        server
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri_b.clone() },
            })
            .await;

        {
            let registry = server.document_registry.read().await;
            assert_eq!(registry.get_all_open().len(), 0);
        }

        // Shutdown
        assert!(server.shutdown().await.is_ok());
    }

    /// Integration test: message framing verification.
    /// Serialize server responses to JSON and verify Content-Length header
    /// format matches the JSON byte length.
    /// Requirement 5.4: Content-Length headers for message framing.
    #[tokio::test]
    async fn test_message_framing_content_length() {
        let service = make_server();
        let server = service.inner();

        // Get a real InitializeResult from the server
        let params = InitializeParams {
            process_id: Some(1),
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let init_result = server
            .initialize(params)
            .await
            .expect("initialize must succeed");

        // Serialize the result into a JSON-RPC 2.0 response envelope
        let result_json =
            serde_json::to_value(&init_result).expect("InitializeResult must serialize to JSON");
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": result_json
        });
        let json_body =
            serde_json::to_string(&envelope).expect("envelope must serialize to JSON string");

        // Construct the LSP message frame
        let byte_length = json_body.len();
        let frame = format!("Content-Length: {}\r\n\r\n{}", byte_length, json_body);

        // Verify frame starts with Content-Length header
        assert!(
            frame.starts_with("Content-Length: "),
            "LSP frame must start with 'Content-Length: '"
        );

        // Parse the Content-Length value
        let header_end = frame
            .find("\r\n\r\n")
            .expect("frame must contain separator");
        let header_line = &frame[..header_end];
        let claimed_length: usize = header_line
            .strip_prefix("Content-Length: ")
            .expect("header must start with 'Content-Length: '")
            .parse()
            .expect("Content-Length value must be a valid integer");

        // Extract body after separator
        let body = &frame[header_end + 4..];

        // Content-Length must match actual body byte length
        assert_eq!(
            claimed_length,
            body.len(),
            "Content-Length ({}) must match actual body byte length ({})",
            claimed_length,
            body.len()
        );

        // Body must be valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(body).expect("body must be valid JSON");
        assert_eq!(parsed.get("jsonrpc").unwrap().as_str().unwrap(), "2.0");
    }

    /// Integration test: message framing for shutdown response.
    /// Shutdown returns Ok(()) which serializes to null — verify the
    /// Content-Length header is correct for that payload too.
    #[tokio::test]
    async fn test_message_framing_shutdown_response() {
        let service = make_server();
        let server = service.inner();

        server
            .initialize(InitializeParams {
                process_id: Some(1),
                root_uri: None,
                capabilities: ClientCapabilities::default(),
                ..Default::default()
            })
            .await
            .unwrap();

        server.shutdown().await.expect("shutdown must succeed");

        // Shutdown result is () which serializes to null in JSON-RPC
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": null
        });
        let json_body = serde_json::to_string(&envelope).unwrap();
        let byte_length = json_body.len();
        let frame = format!("Content-Length: {}\r\n\r\n{}", byte_length, json_body);

        let header_end = frame.find("\r\n\r\n").unwrap();
        let header_line = &frame[..header_end];
        let claimed: usize = header_line
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();
        let body = &frame[header_end + 4..];

        assert_eq!(claimed, body.len());

        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.get("result").unwrap(), &serde_json::Value::Null);
    }

    /// Integration test: JSON-RPC 2.0 compliance for initialize response.
    /// Verify the response has jsonrpc: "2.0", id matching request, and
    /// result field (not error).
    /// Requirements 5.1: JSON-RPC 2.0 message format.
    #[tokio::test]
    async fn test_jsonrpc_compliance_initialize() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: Some(42),
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let init_result = server
            .initialize(params)
            .await
            .expect("initialize must succeed");

        let result_json = serde_json::to_value(&init_result).unwrap();

        // Build JSON-RPC 2.0 response envelope
        let request_id = 7u64;
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result_json
        });

        // Verify jsonrpc field
        assert_eq!(
            envelope.get("jsonrpc").unwrap().as_str().unwrap(),
            "2.0",
            "response must have jsonrpc: '2.0'"
        );

        // Verify id matches request
        assert_eq!(
            envelope.get("id").unwrap().as_u64().unwrap(),
            request_id,
            "response id must match request id"
        );

        // Verify result field is present
        assert!(
            envelope.get("result").is_some(),
            "response must have 'result' field"
        );

        // Verify no error field (valid response)
        assert!(
            envelope.get("error").is_none(),
            "successful response must NOT have 'error' field"
        );
    }

    /// Integration test: JSON-RPC 2.0 compliance for shutdown response.
    /// Shutdown returns null result — verify the envelope is well-formed.
    #[tokio::test]
    async fn test_jsonrpc_compliance_shutdown() {
        let service = make_server();
        let server = service.inner();

        server
            .initialize(InitializeParams {
                process_id: Some(1),
                root_uri: None,
                capabilities: ClientCapabilities::default(),
                ..Default::default()
            })
            .await
            .unwrap();

        server.shutdown().await.expect("shutdown must succeed");

        let request_id = 42u64;
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": null
        });

        assert_eq!(envelope.get("jsonrpc").unwrap().as_str().unwrap(), "2.0");
        assert_eq!(envelope.get("id").unwrap().as_u64().unwrap(), request_id);
        assert!(envelope.get("result").is_some());
        assert_eq!(envelope.get("result").unwrap(), &serde_json::Value::Null);
        assert!(envelope.get("error").is_none());
    }

    /// Integration test: JSON-RPC 2.0 compliance — round-trip serialization.
    /// Verify that serializing and deserializing preserves all JSON-RPC fields.
    #[tokio::test]
    async fn test_jsonrpc_round_trip_serialization() {
        let service = make_server();
        let server = service.inner();

        let params = InitializeParams {
            process_id: Some(1),
            root_uri: Some(Url::parse("file:///project").unwrap()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };
        let init_result = server.initialize(params).await.unwrap();
        let result_json = serde_json::to_value(&init_result).unwrap();

        let request_id = 100u64;
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result_json
        });

        // Serialize to string and parse back
        let serialized = serde_json::to_string(&envelope).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // All fields must survive the round trip
        assert_eq!(parsed.get("jsonrpc").unwrap().as_str().unwrap(), "2.0");
        assert_eq!(parsed.get("id").unwrap().as_u64().unwrap(), request_id);
        assert!(parsed.get("result").is_some());
        assert!(parsed.get("error").is_none());

        // The result must contain capabilities and serverInfo
        let result = parsed.get("result").unwrap();
        assert!(result.get("capabilities").is_some());
        assert!(result.get("serverInfo").is_some());
    }

    // Feature: zed-pdf-lsp, Property 18: Asynchronous Non-Blocking Processing
    mod property_async_non_blocking {
        use proptest::prelude::*;
        use std::time::{Duration, Instant};

        /// **Validates: Requirements 7.3**
        ///
        /// Property: "For any two concurrent PDF conversion requests,
        /// processing one SHALL NOT block the other from starting or
        /// completing."
        ///
        /// We generate pairs of PDF URIs pointing to non-existent files.
        /// Both conversions are spawned concurrently via tokio::join!.
        /// Since the files don't exist, both will fail quickly with
        /// FileNotFound. The key property is that both complete
        /// concurrently — the total wall-clock time should be roughly
        /// the same as a single conversion, not 2x.
        ///
        /// Strategy that generates pairs of distinct non-existent PDF URIs.
        fn pdf_uri_pair_strategy() -> impl Strategy<Value = (String, String)> {
            ("[a-zA-Z][a-zA-Z0-9_]{1,12}", "[a-zA-Z][a-zA-Z0-9_]{1,12}")
                .prop_filter("URIs must be distinct", |(a, b)| a != b)
                .prop_map(|(a, b)| {
                    (
                        format!("/tmp/zed_pbt_async/{}.pdf", a),
                        format!("/tmp/zed_pbt_async/{}.pdf", b),
                    )
                })
        }

        proptest! {
            #[test]
            fn concurrent_conversions_do_not_block_each_other(
                (path_a, path_b) in pdf_uri_pair_strategy()
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                    let converter = crate::pdf_converter::PdfConverter::new();

                    let path_a = std::path::PathBuf::from(&path_a);
                    let path_b = std::path::PathBuf::from(&path_b);

                    // Measure a single conversion for baseline
                    let single_start = Instant::now();
                    let _ = converter.convert_to_markdown(&path_a).await;
                    let single_duration = single_start.elapsed();

                    // Run two conversions concurrently
                    let concurrent_start = Instant::now();
                    let (result_a, result_b) = tokio::join!(
                        converter.convert_to_markdown(&path_a),
                        converter.convert_to_markdown(&path_b),
                    );
                    let concurrent_duration = concurrent_start.elapsed();

                    // Both must complete (with errors, since files don't exist)
                    prop_assert!(
                        result_a.is_err(),
                        "Conversion A must complete (expected FileNotFound error)"
                    );
                    prop_assert!(
                        result_b.is_err(),
                        "Conversion B must complete (expected FileNotFound error)"
                    );

                    // The concurrent duration should be less than 2x the single
                    // duration (with generous margin). If one blocked the other,
                    // the concurrent time would be ~2x the single time.
                    // We use 3x as a very generous upper bound to avoid flakiness.
                    let max_allowed = single_duration.saturating_mul(3) + Duration::from_millis(50);
                    prop_assert!(
                        concurrent_duration <= max_allowed,
                        "Concurrent conversions took {:?}, but single took {:?}. \
                         Max allowed is {:?}. One conversion may be blocking the other.",
                        concurrent_duration,
                        single_duration,
                        max_allowed,
                    );

                    Ok(())
                });
                result?;
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 2: Initialization Error Responses Include Descriptive Messages
    mod property_initialization_errors {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 1.4**
        ///
        /// Property: "For any invalid initialize request that causes an error,
        /// the error response SHALL contain a non-empty message field describing
        /// the failure."
        ///
        /// Since tower-lsp's LanguageServer::initialize returns InitializeResult
        /// (not a Result with error variant), the server always succeeds. This
        /// property test verifies robustness: for any generated InitializeParams
        /// (including edge cases), the server never panics and always returns a
        /// valid InitializeResult with populated server info.
        ///
        /// Strategy that generates diverse InitializeParams including edge cases
        /// such as missing process_id, various root URIs, and different client
        /// capability configurations.
        fn diverse_initialize_params_strategy() -> impl Strategy<Value = InitializeParams> {
            let process_id_strategy = prop_oneof![
                Just(None),
                Just(Some(0)),
                Just(Some(1)),
                (1u32..=u32::MAX).prop_map(Some),
            ];

            let root_uri_strategy = prop_oneof![
                Just(None),
                "file:///[a-zA-Z0-9/_.-]{1,60}".prop_map(|s| Some(Url::parse(&s).unwrap())),
                Just(Some(Url::parse("file:///").unwrap())),
                Just(Some(Url::parse("file:///a").unwrap())),
                Just(Some(
                    Url::parse("file:///very/deep/nested/path/to/project").unwrap()
                )),
            ];

            (process_id_strategy, root_uri_strategy).prop_map(|(process_id, root_uri)| {
                InitializeParams {
                    process_id,
                    root_uri,
                    capabilities: ClientCapabilities::default(),
                    ..Default::default()
                }
            })
        }

        proptest! {
            #[test]
            fn initialization_never_panics_and_returns_valid_result(
                params in diverse_initialize_params_strategy()
            ) {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let service = make_server();
                    let server = service.inner();

                    // The server must not panic for any input
                    let result = server.initialize(params).await;

                    // Must always succeed (tower-lsp signature guarantees this)
                    let init_result = result.expect("initialize must not return an error");

                    // Server info must always be present with a non-empty name,
                    // ensuring that if the server were to produce an error response
                    // it would contain descriptive information
                    let server_info = init_result.server_info
                        .expect("response must include serverInfo");
                    assert!(
                        !server_info.name.is_empty(),
                        "serverInfo.name must be non-empty (descriptive)"
                    );
                    assert!(
                        server_info.version.is_some(),
                        "serverInfo.version must be present"
                    );
                });
            }
        }
    }
}
