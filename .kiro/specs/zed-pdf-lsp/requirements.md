# Requirements Document

## Introduction

The zed-pdf-lsp is a Language Server Protocol (LSP) implementation that enables the Zed editor to read PDF files by converting them to Markdown format. This feature leverages the LSP architecture to treat PDFs as a "language" that gets translated to Markdown, allowing Zed to display PDF content as text without requiring native PDF rendering capabilities.

## Glossary

- **LSP_Server**: The zed-pdf-lsp server process that implements the Language Server Protocol
- **Zed_Editor**: The Zed text editor client that communicates with the LSP_Server
- **PDF_Converter**: The component within LSP_Server that extracts text from PDF files and converts to Markdown
- **LSP_Message**: A JSON-formatted message conforming to the Language Server Protocol specification
- **Document_URI**: A file path or URI identifying a PDF document
- **Markdown_Content**: Text formatted using Markdown syntax

## Requirements

### Requirement 1: LSP Server Initialization

**User Story:** As a Zed user, I want the LSP server to initialize properly, so that PDF viewing capabilities are available when I open the editor.

#### Acceptance Criteria

1. WHEN Zed_Editor sends an initialize request, THE LSP_Server SHALL respond with server capabilities including document synchronization
2. THE LSP_Server SHALL declare support for the "pdf" file extension in its capabilities
3. WHEN initialization is complete, THE LSP_Server SHALL send an initialized notification to Zed_Editor
4. IF initialization fails, THEN THE LSP_Server SHALL return an error response with a descriptive message

### Requirement 2: PDF File Opening

**User Story:** As a Zed user, I want to click on a PDF file in the file tree, so that I can view its contents as Markdown.

#### Acceptance Criteria

1. WHEN Zed_Editor sends a textDocument/didOpen notification with a Document_URI ending in ".pdf", THE LSP_Server SHALL accept the request
2. THE LSP_Server SHALL read the binary PDF file from the provided Document_URI
3. IF the PDF file does not exist, THEN THE LSP_Server SHALL log an error and return empty content
4. IF the PDF file is not readable, THEN THE LSP_Server SHALL log an error and return an error message as Markdown_Content

### Requirement 3: PDF to Markdown Conversion

**User Story:** As a Zed user, I want PDF text content extracted and formatted as Markdown, so that I can read it in the editor.

#### Acceptance Criteria

1. WHEN a PDF file is opened, THE PDF_Converter SHALL extract all text content from the PDF
2. THE PDF_Converter SHALL convert extracted text into valid Markdown_Content
3. THE PDF_Converter SHALL preserve paragraph structure from the original PDF
4. THE PDF_Converter SHALL convert PDF headings to Markdown heading syntax where detectable
5. THE PDF_Converter SHALL handle multi-page PDFs by extracting text from all pages sequentially
6. IF the PDF contains no extractable text, THEN THE PDF_Converter SHALL return a message indicating the PDF is empty or image-based

### Requirement 4: Content Delivery to Editor

**User Story:** As a Zed user, I want the converted Markdown to appear in the editor, so that I can read the PDF content.

#### Acceptance Criteria

1. WHEN PDF_Converter completes conversion, THE LSP_Server SHALL send the Markdown_Content to Zed_Editor
2. THE LSP_Server SHALL format the response as a valid LSP_Message
3. THE LSP_Server SHALL include the complete Markdown_Content in a single response
4. THE Markdown_Content SHALL be valid UTF-8 encoded text

### Requirement 5: LSP Protocol Compliance

**User Story:** As a developer, I want the server to follow LSP specifications, so that it works reliably with Zed and potentially other LSP-compatible editors.

#### Acceptance Criteria

1. THE LSP_Server SHALL communicate using JSON-RPC 2.0 message format
2. THE LSP_Server SHALL implement the textDocument/didOpen notification handler
3. THE LSP_Server SHALL implement the textDocument/didClose notification handler
4. THE LSP_Server SHALL use Content-Length headers for message framing
5. WHEN Zed_Editor sends a shutdown request, THE LSP_Server SHALL respond with null and prepare for exit
6. WHEN Zed_Editor sends an exit notification, THE LSP_Server SHALL terminate the process

### Requirement 6: Error Handling and Logging

**User Story:** As a developer, I want clear error messages and logs, so that I can troubleshoot issues with PDF conversion.

#### Acceptance Criteria

1. WHEN an error occurs during PDF processing, THE LSP_Server SHALL log the error with the Document_URI and error details
2. IF a PDF is corrupted, THEN THE LSP_Server SHALL return Markdown_Content containing an error message
3. IF a PDF uses unsupported encryption, THEN THE LSP_Server SHALL return Markdown_Content indicating the file is encrypted
4. THE LSP_Server SHALL log all incoming LSP_Message requests at debug level
5. THE LSP_Server SHALL log all outgoing LSP_Message responses at debug level

### Requirement 7: Performance Requirements

**User Story:** As a Zed user, I want PDFs to open quickly, so that my workflow is not interrupted.

#### Acceptance Criteria

1. WHEN a PDF file smaller than 10MB is opened, THE LSP_Server SHALL complete conversion within 2 seconds
2. WHEN a PDF file larger than 10MB is opened, THE LSP_Server SHALL complete conversion within 5 seconds
3. THE LSP_Server SHALL process PDF conversion asynchronously to avoid blocking other requests
4. THE LSP_Server SHALL limit memory usage to 500MB per PDF conversion operation

### Requirement 8: Document Lifecycle Management

**User Story:** As a Zed user, I want the server to properly manage document state, so that resources are cleaned up when I close PDF files.

#### Acceptance Criteria

1. WHEN Zed_Editor sends a textDocument/didClose notification, THE LSP_Server SHALL release resources associated with that Document_URI
2. THE LSP_Server SHALL maintain a registry of currently open PDF documents
3. WHEN a document is closed, THE LSP_Server SHALL remove it from the registry within 100ms
4. THE LSP_Server SHALL handle multiple PDF documents being open simultaneously
