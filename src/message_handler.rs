// Message Handler
// This module handles LSP message logging and error formatting

use crate::pdf_converter::ConversionError;
use tracing::debug;

pub struct MessageHandler {
    // Using tracing for structured logging
    // The logger is implicit through tracing macros
}

impl Default for MessageHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler {
    pub fn new() -> Self {
        debug!("MessageHandler initialized");
        Self {}
    }

    pub fn log_request(&self, method: &str, params: &serde_json::Value) {
        debug!(
            method = method,
            params = %params,
            "Received LSP request"
        );
    }

    pub fn log_response(&self, method: &str, result: &serde_json::Value) {
        debug!(
            method = method,
            result = %result,
            "Sending LSP response"
        );
    }

    pub fn format_error_response(&self, error: ConversionError) -> String {
        match error {
            ConversionError::FileNotFound(path) => {
                format!(
                    "# Error: File Not Found\n\n\
                    **File**: {}\n\n\
                    **Reason**: The specified PDF file does not exist at the given path.\n\n\
                    Please check that the file path is correct and the file exists.",
                    path
                )
            }
            ConversionError::FileNotReadable(path) => {
                format!(
                    "# Error: File Not Readable\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF file exists but cannot be read due to permission issues or I/O errors.\n\n\
                    Please check file permissions and ensure the file is not locked by another process.",
                    path
                )
            }
            ConversionError::CorruptedPdf { path, details } => {
                let file_display = if path.is_empty() {
                    "(PDF file)".to_string()
                } else {
                    path
                };
                format!(
                    "# Error: Corrupted PDF\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF file structure is invalid or corrupted. Details: {}\n\n\
                    Please try opening the PDF in another application to verify its integrity, or obtain a new copy of the file.",
                    file_display, details
                )
            }
            ConversionError::EncryptedPdf(path) => {
                let file_display = if path.is_empty() {
                    "(PDF file)".to_string()
                } else {
                    path
                };
                format!(
                    "# Error: Encrypted PDF\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF file is encrypted and requires a password.\n\n\
                    Please decrypt the PDF file before opening it in Zed, or use a PDF reader that supports password-protected files.",
                    file_display
                )
            }
            ConversionError::EmptyPdf(path) => {
                let file_display = if path.is_empty() {
                    "(PDF file)".to_string()
                } else {
                    path
                };
                format!(
                    "# Error: Empty PDF\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF contains no extractable text content. It may be an image-only PDF or a scanned document.\n\n\
                    Consider using OCR (Optical Character Recognition) software to extract text from image-based PDFs.",
                    file_display
                )
            }
            ConversionError::MemoryLimitExceeded(path) => {
                let file_display = if path.is_empty() {
                    "(PDF file)".to_string()
                } else {
                    path
                };
                format!(
                    "# Error: Memory Limit Exceeded\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF conversion process exceeded the memory limit (500MB).\n\n\
                    This PDF may be too large or complex to process. Try opening a smaller PDF file, or consider splitting the PDF into smaller parts.",
                    file_display
                )
            }
            ConversionError::ConversionTimeout { path, timeout_secs } => {
                let file_display = if path.is_empty() {
                    "(PDF file)".to_string()
                } else {
                    path
                };
                format!(
                    "# Error: Conversion Timed Out\n\n\
                    **File**: {}\n\n\
                    **Reason**: The PDF conversion did not complete within {} seconds.\n\n\
                    The PDF may be too large or complex. Try opening a smaller PDF file, or consider splitting the PDF into smaller parts.",
                    file_display, timeout_secs
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_handler_new() {
        // Test that MessageHandler can be instantiated
        let _handler = MessageHandler::new();
        // If we reach here, the constructor succeeded
        // The debug log "MessageHandler initialized" will be emitted
    }

    #[test]
    fn test_log_request() {
        let handler = MessageHandler::new();
        let params = json!({
            "textDocument": {
                "uri": "file:///test.pdf"
            }
        });

        // Should not panic and should log at debug level
        handler.log_request("textDocument/didOpen", &params);
    }

    #[test]
    fn test_log_request_initialize() {
        let handler = MessageHandler::new();
        let params = json!({
            "capabilities": {},
            "rootUri": "file:///workspace"
        });
        handler.log_request("initialize", &params);
    }

    #[test]
    fn test_log_request_shutdown() {
        let handler = MessageHandler::new();
        let params = json!({});
        handler.log_request("shutdown", &params);
    }

    #[test]
    fn test_log_request_did_close() {
        let handler = MessageHandler::new();
        let params = json!({
            "textDocument": {
                "uri": "file:///path/to/doc.pdf"
            }
        });
        handler.log_request("textDocument/didClose", &params);
    }

    #[test]
    fn test_log_response() {
        let handler = MessageHandler::new();
        let result = json!({
            "capabilities": {
                "textDocumentSync": {
                    "openClose": true
                }
            }
        });

        // Should not panic and should log at debug level
        handler.log_response("initialize", &result);
    }

    #[test]
    fn test_log_response_with_null_result() {
        let handler = MessageHandler::new();
        let result = json!(null);

        // Should handle null result gracefully
        handler.log_response("shutdown", &result);
    }

    #[test]
    fn test_log_response_did_open_content() {
        let handler = MessageHandler::new();
        let result = json!({
            "content": "# Converted PDF\n\nSome text."
        });
        handler.log_response("textDocument/didOpen", &result);
    }

    #[test]
    fn test_log_response_did_close() {
        let handler = MessageHandler::new();
        let result = json!(null);
        handler.log_response("textDocument/didClose", &result);
    }

    #[test]
    fn test_log_request_with_empty_params() {
        let handler = MessageHandler::new();
        let params = json!({});

        // Should handle empty params gracefully
        handler.log_request("shutdown", &params);
    }

    #[test]
    fn test_format_error_response_file_not_found() {
        let handler = MessageHandler::new();
        let error = ConversionError::FileNotFound("/path/to/missing.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: File Not Found"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/missing.pdf"));
        // Should contain reason
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("does not exist"));
        // Should contain suggested action
        assert!(markdown.contains("check that the file path is correct"));
    }

    #[test]
    fn test_format_error_response_file_not_readable() {
        let handler = MessageHandler::new();
        let error = ConversionError::FileNotReadable("/path/to/locked.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: File Not Readable"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/locked.pdf"));
        // Should contain reason
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("cannot be read"));
        // Should contain suggested action
        assert!(markdown.contains("check file permissions"));
    }

    #[test]
    fn test_format_error_response_corrupted_pdf() {
        let handler = MessageHandler::new();
        let error = ConversionError::CorruptedPdf {
            path: "/path/to/corrupted.pdf".to_string(),
            details: "Invalid PDF header".to_string(),
        };

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: Corrupted PDF"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/corrupted.pdf"));
        // Should contain reason with details
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("Invalid PDF header"));
        // Should contain suggested action
        assert!(markdown.contains("verify its integrity"));
    }

    #[test]
    fn test_format_error_response_encrypted_pdf() {
        let handler = MessageHandler::new();
        let error = ConversionError::EncryptedPdf("/path/to/encrypted.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: Encrypted PDF"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/encrypted.pdf"));
        // Should contain reason
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("encrypted and requires a password"));
        // Should contain suggested action
        assert!(markdown.contains("decrypt the PDF file"));
    }

    #[test]
    fn test_format_error_response_empty_pdf() {
        let handler = MessageHandler::new();
        let error = ConversionError::EmptyPdf("/path/to/empty.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: Empty PDF"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/empty.pdf"));
        // Should contain reason
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("no extractable text content"));
        // Should contain suggested action
        assert!(markdown.contains("OCR"));
    }

    #[test]
    fn test_format_error_response_memory_limit_exceeded() {
        let handler = MessageHandler::new();
        let error = ConversionError::MemoryLimitExceeded("/path/to/large.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should contain error heading
        assert!(markdown.contains("# Error: Memory Limit Exceeded"));
        // Should contain file path
        assert!(markdown.contains("**File**: /path/to/large.pdf"));
        // Should contain reason
        assert!(markdown.contains("**Reason**:"));
        assert!(markdown.contains("exceeded the memory limit"));
        // Should contain suggested action
        assert!(markdown.contains("too large or complex"));
    }

    #[test]
    fn test_format_error_response_conversion_timeout() {
        let handler = MessageHandler::new();
        let error = ConversionError::ConversionTimeout {
            path: "/path/to/slow.pdf".to_string(),
            timeout_secs: 10,
        };

        let markdown = handler.format_error_response(error);

        assert!(markdown.contains("# Error: Conversion Timed Out"));
        assert!(markdown.contains("**File**: /path/to/slow.pdf"));
        assert!(markdown.contains("10 seconds"));
    }

    #[test]
    fn test_format_error_response_returns_valid_markdown() {
        let handler = MessageHandler::new();

        // Test all error variants return non-empty strings
        let errors: Vec<ConversionError> = vec![
            ConversionError::FileNotFound("/test.pdf".to_string()),
            ConversionError::FileNotReadable("/test.pdf".to_string()),
            ConversionError::CorruptedPdf {
                path: "/test.pdf".to_string(),
                details: "test error".to_string(),
            },
            ConversionError::EncryptedPdf("/test.pdf".to_string()),
            ConversionError::EmptyPdf("/test.pdf".to_string()),
            ConversionError::MemoryLimitExceeded("/test.pdf".to_string()),
            ConversionError::ConversionTimeout {
                path: "/test.pdf".to_string(),
                timeout_secs: 10,
            },
        ];

        for error in errors {
            let markdown = handler.format_error_response(error);

            // Should not be empty
            assert!(!markdown.is_empty());
            // Should start with markdown heading
            assert!(markdown.starts_with("# Error:"));
            // Should contain File and Reason sections
            assert!(markdown.contains("**File**:"));
            assert!(markdown.contains("**Reason**:"));
        }
    }

    #[test]
    fn test_format_error_response_markdown_structure() {
        let handler = MessageHandler::new();
        let error = ConversionError::FileNotFound("/test.pdf".to_string());

        let markdown = handler.format_error_response(error);

        // Should have proper markdown structure with blank lines
        assert!(markdown.contains("\n\n"));
        // Should have heading at the start
        assert!(markdown.starts_with("# Error:"));
        // Should have bold formatting for labels
        assert!(markdown.contains("**File**:"));
        assert!(markdown.contains("**Reason**:"));
    }

    // ── Unit tests for logging (Task 12.3) ─────────────────────────────
    // Validates: Requirements 6.1, 6.4, 6.5
    mod logging_unit_tests {
        use super::*;
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        /// A writer that captures tracing output into a shared buffer.
        #[derive(Clone)]
        struct TestWriter(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for TestWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }

        impl<'a> MakeWriter<'a> for TestWriter {
            type Writer = TestWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        /// Helper: build a scoped tracing subscriber that writes to the buffer.
        fn with_captured_logs<F>(f: F) -> String
        where
            F: FnOnce(),
        {
            let buf = Arc::new(Mutex::new(Vec::new()));
            let writer = TestWriter(buf.clone());
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .with_ansi(false)
                .with_writer(writer)
                .finish();

            tracing::subscriber::with_default(subscriber, f);

            let captured = buf.lock().unwrap().clone();
            String::from_utf8(captured).unwrap()
        }

        // Requirement 6.4: log_request produces DEBUG level output containing the method name
        #[test]
        fn log_request_emits_debug_with_method_did_open() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_request(
                    "textDocument/didOpen",
                    &json!({"textDocument": {"uri": "file:///test.pdf"}}),
                );
            });
            assert!(
                output.contains("DEBUG"),
                "log_request must emit at DEBUG level, got: {}",
                output
            );
            assert!(
                output.contains("textDocument/didOpen"),
                "log must contain method name, got: {}",
                output
            );
            assert!(
                output.contains("Received LSP request"),
                "log must contain request message, got: {}",
                output
            );
        }

        #[test]
        fn log_request_emits_debug_with_method_initialize() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_request("initialize", &json!({"capabilities": {}}));
            });
            assert!(
                output.contains("DEBUG"),
                "log_request must emit at DEBUG level"
            );
            assert!(
                output.contains("initialize"),
                "log must contain method name 'initialize'"
            );
        }

        #[test]
        fn log_request_emits_debug_with_method_shutdown() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_request("shutdown", &json!({}));
            });
            assert!(
                output.contains("DEBUG"),
                "log_request must emit at DEBUG level"
            );
            assert!(
                output.contains("shutdown"),
                "log must contain method name 'shutdown'"
            );
        }

        // Requirement 6.5: log_response produces DEBUG level output containing the method name
        #[test]
        fn log_response_emits_debug_with_method_initialize() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_response(
                    "initialize",
                    &json!({"capabilities": {"textDocumentSync": {"openClose": true}}}),
                );
            });
            assert!(
                output.contains("DEBUG"),
                "log_response must emit at DEBUG level, got: {}",
                output
            );
            assert!(
                output.contains("initialize"),
                "log must contain method name, got: {}",
                output
            );
            assert!(
                output.contains("Sending LSP response"),
                "log must contain response message, got: {}",
                output
            );
        }

        #[test]
        fn log_response_emits_debug_with_method_did_open() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_response(
                    "textDocument/didOpen",
                    &json!({"status": "success", "pages": 3}),
                );
            });
            assert!(
                output.contains("DEBUG"),
                "log_response must emit at DEBUG level"
            );
            assert!(
                output.contains("textDocument/didOpen"),
                "log must contain method name"
            );
        }

        #[test]
        fn log_response_emits_debug_with_null_result() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_response("shutdown", &json!(null));
            });
            assert!(
                output.contains("DEBUG"),
                "log_response must emit at DEBUG level"
            );
            assert!(
                output.contains("shutdown"),
                "log must contain method name 'shutdown'"
            );
        }

        // Requirement 6.1: error formatting includes document URI and error details
        #[test]
        fn format_error_includes_uri_for_file_not_found() {
            let handler = MessageHandler::new();
            let output = handler.format_error_response(ConversionError::FileNotFound(
                "/home/user/docs/report.pdf".to_string(),
            ));
            assert!(
                output.contains("/home/user/docs/report.pdf"),
                "error must include document URI"
            );
            assert!(
                output.contains("**Reason**:"),
                "error must include error details"
            );
            assert!(
                output.contains("does not exist"),
                "error must describe the problem"
            );
        }

        #[test]
        fn format_error_includes_uri_for_corrupted_pdf() {
            let handler = MessageHandler::new();
            let output = handler.format_error_response(ConversionError::CorruptedPdf {
                path: "/data/broken.pdf".to_string(),
                details: "Invalid cross-reference table".to_string(),
            });
            assert!(
                output.contains("/data/broken.pdf"),
                "error must include document URI"
            );
            assert!(
                output.contains("Invalid cross-reference table"),
                "error must include specific details"
            );
        }

        #[test]
        fn format_error_includes_uri_for_encrypted_pdf() {
            let handler = MessageHandler::new();
            let output = handler.format_error_response(ConversionError::EncryptedPdf(
                "/secure/secret.pdf".to_string(),
            ));
            assert!(
                output.contains("/secure/secret.pdf"),
                "error must include document URI"
            );
            assert!(
                output.contains("encrypted"),
                "error must describe encryption issue"
            );
        }

        #[test]
        fn format_error_includes_uri_for_timeout() {
            let handler = MessageHandler::new();
            let output = handler.format_error_response(ConversionError::ConversionTimeout {
                path: "/large/huge.pdf".to_string(),
                timeout_secs: 10,
            });
            assert!(
                output.contains("/large/huge.pdf"),
                "error must include document URI"
            );
            assert!(
                output.contains("10 seconds"),
                "error must include timeout details"
            );
        }

        // Verify log levels: DEBUG for messages, DEBUG for MessageHandler::new()
        #[test]
        fn message_handler_new_emits_debug_log() {
            let output = with_captured_logs(|| {
                let _handler = MessageHandler::new();
            });
            assert!(
                output.contains("DEBUG"),
                "MessageHandler::new() must emit at DEBUG level, got: {}",
                output
            );
            assert!(
                output.contains("MessageHandler initialized"),
                "must log initialization message"
            );
        }

        #[test]
        fn log_request_does_not_emit_error_or_info() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_request("textDocument/didOpen", &json!({"uri": "file:///test.pdf"}));
            });
            // Filter out the DEBUG line from MessageHandler::new() and check request lines
            let request_lines: Vec<&str> = output
                .lines()
                .filter(|l| l.contains("Received LSP request"))
                .collect();
            assert!(!request_lines.is_empty(), "must have request log line");
            for line in &request_lines {
                assert!(
                    line.contains("DEBUG"),
                    "request log must be DEBUG, not ERROR/INFO: {}",
                    line
                );
                assert!(
                    !line.contains("ERROR"),
                    "request log must not be ERROR: {}",
                    line
                );
            }
        }

        #[test]
        fn log_response_does_not_emit_error_or_info() {
            let output = with_captured_logs(|| {
                let handler = MessageHandler::new();
                handler.log_response("initialize", &json!({"capabilities": {}}));
            });
            let response_lines: Vec<&str> = output
                .lines()
                .filter(|l| l.contains("Sending LSP response"))
                .collect();
            assert!(!response_lines.is_empty(), "must have response log line");
            for line in &response_lines {
                assert!(
                    line.contains("DEBUG"),
                    "response log must be DEBUG, not ERROR/INFO: {}",
                    line
                );
                assert!(
                    !line.contains("ERROR"),
                    "response log must not be ERROR: {}",
                    line
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 15: Error Logging Includes Context
    mod property_error_logging {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 6.1**
        ///
        /// Property: "For any error during PDF processing, the log entry SHALL
        /// contain both the Document_URI and error details."
        fn error_with_path_strategy() -> impl Strategy<Value = (ConversionError, String)> {
            let details_strategy = "[a-zA-Z0-9 ._-]{1,30}";

            prop_oneof![
                "/[a-zA-Z0-9/_.-]{1,40}\\.pdf".prop_map(|p: String| {
                    let path = p.clone();
                    (ConversionError::FileNotFound(p), path)
                }),
                "/[a-zA-Z0-9/_.-]{1,40}\\.pdf".prop_map(|p: String| {
                    let path = p.clone();
                    (ConversionError::FileNotReadable(p), path)
                }),
                ("/[a-zA-Z0-9/_.-]{1,40}\\.pdf", details_strategy).prop_map(|(p, d)| {
                    let path = p.clone();
                    (
                        ConversionError::CorruptedPdf {
                            path: p,
                            details: d,
                        },
                        path,
                    )
                }),
                "/[a-zA-Z0-9/_.-]{1,40}\\.pdf".prop_map(|p: String| {
                    let path = p.clone();
                    (ConversionError::EncryptedPdf(p), path)
                }),
                "/[a-zA-Z0-9/_.-]{1,40}\\.pdf".prop_map(|p: String| {
                    let path = p.clone();
                    (ConversionError::EmptyPdf(p), path)
                }),
                "/[a-zA-Z0-9/_.-]{1,40}\\.pdf".prop_map(|p: String| {
                    let path = p.clone();
                    (ConversionError::MemoryLimitExceeded(p), path)
                }),
                ("/[a-zA-Z0-9/_.-]{1,40}\\.pdf", 1u64..3600).prop_map(|(p, t)| {
                    let path = p.clone();
                    (
                        ConversionError::ConversionTimeout {
                            path: p,
                            timeout_secs: t,
                        },
                        path,
                    )
                }),
            ]
        }

        proptest! {
            #[test]
            fn error_output_contains_uri_and_details(
                (error, expected_path) in error_with_path_strategy()
            ) {
                let handler = MessageHandler::new();
                let output = handler.format_error_response(error);

                // The output must contain the Document_URI (file path)
                prop_assert!(
                    output.contains(&expected_path),
                    "Error output must contain the Document_URI '{}', got:\n{}",
                    expected_path,
                    output
                );

                // The output must contain error-specific details (the Reason section)
                prop_assert!(
                    output.contains("**Reason**:"),
                    "Error output must contain error details via '**Reason**:' section"
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 16: Bidirectional Message Logging
    mod property_message_logging {
        use super::*;
        use proptest::prelude::*;
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        /// A writer that captures tracing output into a shared buffer.
        #[derive(Clone)]
        struct TestWriter(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for TestWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }

        impl<'a> MakeWriter<'a> for TestWriter {
            type Writer = TestWriter;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        /// **Validates: Requirements 6.4, 6.5**
        ///
        /// Property: "For any LSP message (incoming request or outgoing response),
        /// a debug-level log entry SHALL be created."
        fn method_strategy() -> impl Strategy<Value = String> {
            prop_oneof![
                Just("initialize".to_string()),
                Just("shutdown".to_string()),
                Just("textDocument/didOpen".to_string()),
                Just("textDocument/didClose".to_string()),
                "[a-zA-Z/]{1,30}",
            ]
        }

        fn json_value_strategy() -> impl Strategy<Value = serde_json::Value> {
            prop_oneof![
                Just(serde_json::Value::Null),
                Just(json!({})),
                "[a-zA-Z0-9 ]{0,20}".prop_map(|s| json!({ "data": s })),
                (0i64..1000).prop_map(|n| json!({ "id": n })),
                Just(json!({"textDocument": {"uri": "file:///test.pdf"}})),
            ]
        }

        /// Helper: build a scoped tracing subscriber that writes to the buffer.
        fn with_captured_logs<F>(f: F) -> String
        where
            F: FnOnce(),
        {
            let buf = Arc::new(Mutex::new(Vec::new()));
            let writer = TestWriter(buf.clone());
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .with_ansi(false)
                .with_writer(writer)
                .finish();

            tracing::subscriber::with_default(subscriber, f);

            let captured = buf.lock().unwrap().clone();
            String::from_utf8(captured).unwrap()
        }

        proptest! {
            #[test]
            fn log_request_creates_debug_entry(
                method in method_strategy(),
                params in json_value_strategy()
            ) {
                let method_clone = method.clone();
                let output = with_captured_logs(|| {
                    let handler = MessageHandler::new();
                    handler.log_request(&method_clone, &params);
                });

                // A debug-level log entry must be produced
                prop_assert!(
                    output.contains("DEBUG"),
                    "Expected DEBUG-level log entry for request '{}', got: '{}'",
                    method, output
                );
                // The log must mention the request message text
                prop_assert!(
                    output.contains("Received LSP request"),
                    "Log must contain 'Received LSP request', got: '{}'",
                    output
                );
                // The log must include the method name
                prop_assert!(
                    output.contains(&method),
                    "Log must contain method name '{}', got: '{}'",
                    method, output
                );
            }

            #[test]
            fn log_response_creates_debug_entry(
                method in method_strategy(),
                result in json_value_strategy()
            ) {
                let method_clone = method.clone();
                let output = with_captured_logs(|| {
                    let handler = MessageHandler::new();
                    handler.log_response(&method_clone, &result);
                });

                // A debug-level log entry must be produced
                prop_assert!(
                    output.contains("DEBUG"),
                    "Expected DEBUG-level log entry for response '{}', got: '{}'",
                    method, output
                );
                // The log must mention the response message text
                prop_assert!(
                    output.contains("Sending LSP response"),
                    "Log must contain 'Sending LSP response', got: '{}'",
                    output
                );
                // The log must include the method name
                prop_assert!(
                    output.contains(&method),
                    "Log must contain method name '{}', got: '{}'",
                    method, output
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 5: Error Handling Returns Markdown Messages
    mod property_error_handling {
        use super::*;
        use proptest::prelude::*;

        /// **Validates: Requirements 2.3, 2.4, 6.2, 6.3**
        ///
        /// Property: "For any error condition (missing file, unreadable file, corrupted PDF,
        /// encrypted PDF), the server SHALL return content formatted as valid Markdown
        /// containing an error description."
        fn error_strategy() -> impl Strategy<Value = ConversionError> {
            let path_strategy = "[a-zA-Z0-9/_.-]{1,50}";
            let details_strategy = "[a-zA-Z0-9 ._-]{1,50}";

            prop_oneof![
                path_strategy.prop_map(ConversionError::FileNotFound),
                path_strategy.prop_map(ConversionError::FileNotReadable),
                (path_strategy, details_strategy).prop_map(|(p, d)| {
                    ConversionError::CorruptedPdf {
                        path: p,
                        details: d,
                    }
                }),
                path_strategy.prop_map(ConversionError::EncryptedPdf),
                path_strategy.prop_map(ConversionError::EmptyPdf),
                path_strategy.prop_map(ConversionError::MemoryLimitExceeded),
                (path_strategy, 1u64..3600).prop_map(|(p, t)| ConversionError::ConversionTimeout {
                    path: p,
                    timeout_secs: t
                }),
            ]
        }

        proptest! {
            #[test]
            fn error_response_is_valid_markdown(error in error_strategy()) {
                let handler = MessageHandler::new();
                let output = handler.format_error_response(error);

                prop_assert!(!output.is_empty(), "Output must be non-empty");
                prop_assert!(
                    output.starts_with("# Error:"),
                    "Output must start with Markdown heading '# Error:', got: {}",
                    &output[..output.len().min(40)]
                );
                prop_assert!(
                    output.contains("**File**:"),
                    "Output must contain '**File**:' section"
                );
                prop_assert!(
                    output.contains("**Reason**:"),
                    "Output must contain '**Reason**:' section"
                );
            }
        }
    }
}
