# Implementation Plan: zed-pdf-lsp

## Overview

This plan implements a Language Server Protocol (LSP) server in Rust that enables Zed editor to display PDF files by converting them to Markdown. The implementation follows a modular architecture with LSP Server Core, Message Handler, Document Registry, and PDF Converter components. The server uses tower-lsp for LSP framework, tokio for async runtime, and pdf-extract for PDF text extraction.

## Tasks

- [x] 1. Set up project structure and dependencies
  - Create Rust project with cargo init
  - Add dependencies: tower-lsp, tokio, pdf-extract (or lopdf), serde_json, tracing, proptest
  - Configure Cargo.toml with async features
  - Set up basic project structure with src/main.rs and module files
  - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6_

- [x] 2. Implement core data structures and types
  - [x] 2.1 Create DocumentRegistry with thread-safe HashMap
    - Implement DocumentRegistry struct with Arc<RwLock<HashMap>>
    - Implement DocumentState struct with uri, opened_at, content_hash fields
    - Add register, unregister, is_open, get_all_open methods
    - _Requirements: 8.2_

  - [x] 2.2 Create ConversionError enum
    - Define all error variants: FileNotFound, FileNotReadable, CorruptedPdf, EncryptedPdf, EmptyPdf, MemoryLimitExceeded
    - Implement Display and Error traits
    - _Requirements: 2.3, 2.4, 6.2, 6.3_

  - [x] 2.3 Create ConversionResult struct
    - Define fields: content (String), page_count (usize), conversion_time_ms (u64)
    - _Requirements: 3.1, 3.2_


- [x] 3. Implement PDF Converter component
  - [x] 3.1 Create PdfConverter struct with configuration
    - Define PdfConverter with max_memory_mb field
    - Implement new() constructor with default memory limit (500MB)
    - _Requirements: 7.4_

  - [x] 3.2 Implement PDF text extraction
    - Implement extract_text method to read PDF binary and extract text per page
    - Handle multi-page PDFs by iterating through all pages
    - Return Vec<String> with one entry per page
    - _Requirements: 3.1, 3.5_

  - [x] 3.3 Implement heading detection
    - Implement detect_headings method to identify headings based on font size/style
    - Return Vec of (line_number, heading_text) tuples
    - _Requirements: 3.4_

  - [x] 3.4 Implement Markdown formatting
    - Implement format_as_markdown method to convert page text to Markdown
    - Preserve paragraph structure with blank lines
    - Convert detected headings to Markdown heading syntax (# or ##)
    - Add page separators (---) between pages
    - _Requirements: 3.2, 3.3, 3.4_

  - [x] 3.5 Implement main conversion method
    - Implement convert_to_markdown async method
    - Read PDF file from path, call extract_text, detect_headings, format_as_markdown
    - Measure conversion time and return ConversionResult
    - Handle all error cases: file not found, not readable, corrupted, encrypted, empty
    - Return appropriate ConversionError variants
    - _Requirements: 2.2, 2.3, 2.4, 3.1, 3.2, 3.6_

  - [x] 3.6 Write property test for text extraction
    - **Property 6: Text Extraction from Valid PDFs**
    - **Validates: Requirements 3.1**

  - [x] 3.7 Write property test for Markdown validity
    - **Property 7: Markdown Output Validity**
    - **Validates: Requirements 3.2**

  - [x] 3.8 Write property test for paragraph preservation
    - **Property 8: Paragraph Structure Preservation**
    - **Validates: Requirements 3.3**

  - [x] 3.9 Write property test for heading conversion
    - **Property 9: Heading Detection and Conversion**
    - **Validates: Requirements 3.4**

  - [x] 3.10 Write property test for multi-page extraction
    - **Property 10: Multi-Page Content Extraction**
    - **Validates: Requirements 3.5**

  - [x] 3.11 Write property test for UTF-8 encoding
    - **Property 11: UTF-8 Encoding Validity**
    - **Validates: Requirements 4.4**

  - [x] 3.12 Write property test for error handling
    - **Property 5: Error Handling Returns Markdown Messages**
    - **Validates: Requirements 2.3, 2.4, 6.2, 6.3**

  - [x] 3.13 Write unit tests for PDF converter
    - Test single-page PDF conversion
    - Test multi-page PDF conversion
    - Test empty PDF handling
    - Test corrupted PDF error
    - Test encrypted PDF error
    - _Requirements: 3.1, 3.2, 3.5, 3.6_


- [x] 4. Implement Message Handler component
  - [x] 4.1 Create MessageHandler struct with logger
    - Define MessageHandler with tracing logger
    - Implement new() constructor
    - _Requirements: 6.4, 6.5_

  - [x] 4.2 Implement request/response logging
    - Implement log_request method to log incoming LSP messages at debug level
    - Implement log_response method to log outgoing LSP messages at debug level
    - Include method name and relevant parameters in logs
    - _Requirements: 6.4, 6.5_

  - [x] 4.3 Implement error formatting
    - Implement format_error_response method to convert ConversionError to Markdown
    - Use error message template with File, Reason, and suggested action
    - _Requirements: 2.3, 2.4, 6.1, 6.2, 6.3_

  - [x] 4.4 Write property test for error logging
    - **Property 15: Error Logging Includes Context**
    - **Validates: Requirements 6.1**

  - [x] 4.5 Write property test for message logging
    - **Property 16: Bidirectional Message Logging**
    - **Validates: Requirements 6.4, 6.5**

  - [x] 4.6 Write unit tests for message handler
    - Test log_request with various LSP methods
    - Test log_response with various results
    - Test format_error_response for each ConversionError variant
    - _Requirements: 6.1, 6.4, 6.5_

- [x] 5. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.


- [x] 6. Implement LSP Server Core
  - [x] 6.1 Create PdfLspServer struct
    - Define PdfLspServer with client, document_registry, pdf_converter, message_handler fields
    - Use Arc for shared ownership of registry and converter
    - _Requirements: 1.1, 5.1_

  - [x] 6.2 Implement initialize handler
    - Implement initialize method to handle LSP initialize request
    - Return InitializeResult with server capabilities
    - Include textDocumentSync with openClose: true
    - Declare support for "pdf" file extension
    - Log initialization at info level
    - _Requirements: 1.1, 1.2_

  - [x] 6.3 Implement initialized notification handler
    - Implement initialized method to handle initialized notification
    - Log server ready state
    - _Requirements: 1.3_

  - [x] 6.4 Implement shutdown handler
    - Implement shutdown method to return null result
    - Prepare server for graceful termination
    - _Requirements: 5.5_

  - [x] 6.5 Implement exit handler
    - Implement exit notification handler to terminate process
    - _Requirements: 5.6_

  - [x] 6.6 Write property test for initialize response
    - **Property 1: Initialize Response Contains Required Capabilities**
    - **Validates: Requirements 1.1**

  - [x] 6.7 Write property test for initialization errors
    - **Property 2: Initialization Error Responses Include Descriptive Messages**
    - **Validates: Requirements 1.4**

  - [x] 6.8 Write property test for shutdown response
    - **Property 14: Shutdown Response Format**
    - **Validates: Requirements 5.5**

  - [x] 6.9 Write unit tests for LSP lifecycle
    - Test initialize → initialized sequence
    - Test shutdown → exit sequence
    - Test initialize with invalid parameters
    - _Requirements: 1.1, 1.3, 1.4, 5.5, 5.6_


- [x] 7. Implement document lifecycle handlers
  - [x] 7.1 Implement didOpen handler
    - Implement did_open method to handle textDocument/didOpen notification
    - Extract document URI from params
    - Validate URI ends with .pdf extension
    - Register document in DocumentRegistry
    - Call pdf_converter.convert_to_markdown asynchronously
    - Send converted Markdown content to client
    - Handle conversion errors by sending error message as Markdown
    - Log all operations with message_handler
    - _Requirements: 2.1, 2.2, 3.1, 4.1, 4.2, 4.3_

  - [x] 7.2 Implement didClose handler
    - Implement did_close method to handle textDocument/didClose notification
    - Extract document URI from params
    - Unregister document from DocumentRegistry
    - Ensure cleanup completes within 100ms
    - Log close operation
    - _Requirements: 8.1, 8.3_

  - [x] 7.3 Write property test for PDF URI acceptance
    - **Property 3: PDF URI Acceptance**
    - **Validates: Requirements 2.1**

  - [x] 7.4 Write property test for file reading attempt
    - **Property 4: File Reading Attempt**
    - **Validates: Requirements 2.2**

  - [x] 7.5 Write property test for resource cleanup
    - **Property 20: Resource Cleanup on Close**
    - **Validates: Requirements 8.1**

  - [x] 7.6 Write property test for registry removal timing
    - **Property 21: Registry Removal Timing**
    - **Validates: Requirements 8.3**

  - [x] 7.7 Write property test for concurrent document handling
    - **Property 22: Concurrent Document Handling**
    - **Validates: Requirements 8.2, 8.4**

  - [x] 7.8 Write unit tests for document lifecycle
    - Test didOpen with valid PDF file
    - Test didOpen with non-existent file
    - Test didOpen with non-PDF extension
    - Test didClose after didOpen
    - Test multiple documents open simultaneously
    - _Requirements: 2.1, 2.2, 2.3, 8.1, 8.2, 8.3, 8.4_


- [x] 8. Implement JSON-RPC message handling
  - [x] 8.1 Set up tower-lsp server with stdin/stdout transport
    - Configure LspService with PdfLspServer
    - Set up Server with stdin/stdout using tower-lsp
    - Implement Content-Length header framing (handled by tower-lsp)
    - _Requirements: 5.1, 5.4_

  - [x] 8.2 Implement main entry point
    - Create main() function with tokio runtime
    - Initialize tracing subscriber for logging
    - Create PdfLspServer instance
    - Start LSP server and await completion
    - _Requirements: 5.1, 6.4, 6.5_

  - [x] 8.3 Write property test for JSON-RPC format
    - **Property 12: JSON-RPC 2.0 Message Format**
    - **Validates: Requirements 5.1, 4.2**

  - [x] 8.4 Write property test for Content-Length headers
    - **Property 13: Content-Length Header Presence**
    - **Validates: Requirements 5.4**

  - [x] 8.5 Write integration tests for full LSP flow
    - Test complete initialize → didOpen → didClose → shutdown → exit flow
    - Test message framing with Content-Length headers
    - Test JSON-RPC 2.0 format compliance
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6_

- [x] 9. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.


- [x] 10. Implement performance optimizations
  - [x] 10.1 Add async processing for PDF conversion
    - Ensure convert_to_markdown runs on tokio thread pool
    - Use tokio::spawn for non-blocking conversion
    - Implement timeout mechanism (10 seconds per conversion)
    - _Requirements: 7.3_

  - [x] 10.2 Implement memory limit enforcement
    - Add memory tracking during PDF conversion
    - Return MemoryLimitExceeded error if limit exceeded
    - _Requirements: 7.4_

  - [x] 10.3 Add conversion time tracking
    - Measure conversion duration using Instant
    - Include timing in ConversionResult
    - Log conversion time at info level
    - _Requirements: 7.1, 7.2_

  - [x] 10.4 Write property test for conversion performance
    - **Property 17: Conversion Time Performance**
    - **Validates: Requirements 7.1, 7.2**

  - [x] 10.5 Write property test for async non-blocking processing
    - **Property 18: Asynchronous Non-Blocking Processing**
    - **Validates: Requirements 7.3**

  - [x] 10.6 Write property test for memory usage limit
    - **Property 19: Memory Usage Limit**
    - **Validates: Requirements 7.4**

  - [x] 10.7 Write performance benchmarks
    - Benchmark conversion time vs file size
    - Benchmark memory usage vs file size
    - Benchmark concurrent document handling
    - Benchmark registry operation latency
    - _Requirements: 7.1, 7.2, 7.3, 7.4_


- [x] 11. Implement comprehensive error handling
  - [x] 11.1 Add error context to all error paths
    - Ensure all ConversionError variants include document URI
    - Add error context using anyhow or similar
    - _Requirements: 6.1_

  - [x] 11.2 Implement graceful degradation for partial failures
    - Handle partial page extraction failures
    - Include error markers in Markdown for failed pages
    - Continue processing remaining pages
    - _Requirements: 3.5_

  - [x] 11.3 Add timeout handling
    - Implement 10-second timeout per conversion
    - Return partial results if available on timeout
    - Log timeout events at warning level
    - _Requirements: 7.1, 7.2_

  - [x] 11.4 Write unit tests for error scenarios
    - Test file not found error
    - Test file not readable error
    - Test corrupted PDF error
    - Test encrypted PDF error
    - Test empty PDF error
    - Test memory limit exceeded error
    - Test timeout error
    - _Requirements: 2.3, 2.4, 6.2, 6.3, 7.4_

- [x] 12. Implement logging infrastructure
  - [x] 12.1 Set up tracing subscriber
    - Configure tracing with appropriate log levels
    - Set up log format: [timestamp] [level] [component] message key=value
    - Enable debug logging for development
    - _Requirements: 6.4, 6.5_

  - [x] 12.2 Add structured logging throughout codebase
    - Log server lifecycle events at info level
    - Log document open/close at info level
    - Log conversion timing at info level
    - Log all errors at error level
    - Log protocol violations at warn level
    - _Requirements: 6.1, 6.4, 6.5_

  - [x] 12.3 Write unit tests for logging
    - Test log output for various operations
    - Verify log levels are correct
    - Verify log context includes required fields
    - _Requirements: 6.1, 6.4, 6.5_

- [x] 13. Final checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.


- [x] 14. Create build and deployment configuration
  - [x] 14.1 Configure Cargo.toml for release builds
    - Set optimization level for release builds
    - Configure binary name and metadata
    - _Requirements: 5.1_

  - [x] 14.2 Create README with installation instructions
    - Document how to build the LSP server
    - Document how to configure Zed to use the server
    - Include example configuration for Zed settings
    - _Requirements: 1.1, 1.2_

  - [x] 14.3 Add CI configuration
    - Set up GitHub Actions or similar for automated testing
    - Run unit tests, property tests, and integration tests
    - Generate coverage reports
    - Run linting and formatting checks
    - _Requirements: All_

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation at key milestones
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The implementation uses Rust with tower-lsp, tokio, pdf-extract, and proptest
- All 22 correctness properties from the design document have corresponding property test tasks
