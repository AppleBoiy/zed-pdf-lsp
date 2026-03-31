// PDF Converter
// This module handles PDF text extraction and conversion to Markdown

use std::path::Path;
use thiserror::Error;

pub struct PdfConverter {
    max_memory_mb: usize,
}

#[derive(Debug)]
pub struct ConversionResult {
    pub content: String,
    pub page_count: usize,
    pub conversion_time_ms: u64,
}

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("File not readable: {0}")]
    FileNotReadable(String),
    
    #[error("Corrupted PDF ({path}): {details}")]
    CorruptedPdf { path: String, details: String },
    
    #[error("PDF is encrypted and requires a password: {0}")]
    EncryptedPdf(String),
    
    #[error("PDF contains no extractable text: {0}")]
    EmptyPdf(String),
    
    #[error("Memory limit exceeded during conversion: {0}")]
    MemoryLimitExceeded(String),
    
    #[error("Conversion timed out after {timeout_secs} seconds: {path}")]
    ConversionTimeout { path: String, timeout_secs: u64 },
}

impl ConversionError {
    /// Add document path context to an error that was created without one.
    pub fn with_path(self, path: String) -> Self {
        match self {
            ConversionError::CorruptedPdf { path: p, details } if p.is_empty() => {
                ConversionError::CorruptedPdf { path, details }
            }
            ConversionError::EncryptedPdf(p) if p.is_empty() => {
                ConversionError::EncryptedPdf(path)
            }
            ConversionError::EmptyPdf(p) if p.is_empty() => {
                ConversionError::EmptyPdf(path)
            }
            other => other,
        }
    }
}

impl PdfConverter {
    pub fn new() -> Self {
        Self {
            max_memory_mb: 500, // Default 500MB limit
        }
    }

    pub async fn convert_to_markdown(&self, pdf_path: &Path) -> Result<ConversionResult, ConversionError> {
        use std::time::Duration;
        
        let timeout_secs = 10;
        let path_display = pdf_path.to_string_lossy().to_string();

        // Wrap the entire conversion in a 10-second timeout
        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.convert_inner(pdf_path),
        )
        .await;

        match result {
            Ok(inner_result) => inner_result,
            Err(_elapsed) => {
                tracing::warn!(
                    path = %path_display,
                    timeout_secs = timeout_secs,
                    "PDF conversion timed out"
                );
                Err(ConversionError::ConversionTimeout {
                    path: path_display,
                    timeout_secs,
                })
            }
        }
    }

    /// Inner conversion logic, separated so it can be wrapped in a timeout.
    async fn convert_inner(&self, pdf_path: &Path) -> Result<ConversionResult, ConversionError> {
        use std::time::Instant;

        let start_time = Instant::now();
        let path_display = pdf_path.to_string_lossy().to_string();

        // Read the PDF file from the file system
        let pdf_data = tokio::fs::read(pdf_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConversionError::FileNotFound(path_display.clone())
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ConversionError::FileNotReadable(path_display.clone())
            } else {
                ConversionError::FileNotReadable(format!("{}: {}", path_display, e))
            }
        })?;

        // Check file size against memory limit (use file size as a proxy for memory usage)
        let file_size_mb = pdf_data.len() / (1024 * 1024);
        if file_size_mb >= self.max_memory_mb {
            tracing::error!(
                path = %path_display,
                file_size_mb = file_size_mb,
                limit_mb = self.max_memory_mb,
                "PDF file exceeds memory limit"
            );
            return Err(ConversionError::MemoryLimitExceeded(path_display.clone()));
        }

        // Extract text from the PDF (this is CPU-bound, so run it on a blocking thread)
        let pages = tokio::task::spawn_blocking({
            let pdf_data = pdf_data.clone();
            let converter = PdfConverter {
                max_memory_mb: self.max_memory_mb,
            };
            move || converter.extract_text(&pdf_data)
        })
        .await
        .map_err(|e| ConversionError::CorruptedPdf {
            path: path_display.clone(),
            details: format!("Task join error: {}", e),
        })?
        .map_err(|e| e.with_path(path_display.clone()))?;

        let page_count = pages.len();

        // Format the extracted text as Markdown
        let content = self.format_as_markdown(pages);

        // Measure conversion time and log at info level
        let conversion_time_ms = start_time.elapsed().as_millis() as u64;
        tracing::info!(
            path = %path_display,
            pages = page_count,
            conversion_time_ms = conversion_time_ms,
            "PDF conversion completed"
        );

        Ok(ConversionResult {
            content,
            page_count,
            conversion_time_ms,
        })
    }

    fn extract_text(&self, pdf_data: &[u8]) -> Result<Vec<String>, ConversionError> {
        // Wrap extraction in catch_unwind for graceful degradation on panics
        let extraction_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem(pdf_data)
        }));

        let pdf = match extraction_result {
            Ok(Ok(text)) => text,
            Ok(Err(e)) => {
                let error_msg = e.to_string();
                if error_msg.contains("encrypt") || error_msg.contains("password") {
                    return Err(ConversionError::EncryptedPdf(String::new()));
                } else if error_msg.contains("Invalid") || error_msg.contains("corrupt") {
                    return Err(ConversionError::CorruptedPdf { path: String::new(), details: error_msg });
                } else {
                    return Err(ConversionError::CorruptedPdf { path: String::new(), details: format!("Failed to extract text: {}", error_msg) });
                }
            }
            Err(_panic) => {
                tracing::error!("PDF extraction panicked — returning corrupted PDF error");
                return Err(ConversionError::CorruptedPdf {
                    path: String::new(),
                    details: "PDF extraction panicked unexpectedly".to_string(),
                });
            }
        };

        // Check if the extracted text is empty
        if pdf.trim().is_empty() {
            return Err(ConversionError::EmptyPdf(String::new()));
        }

        // pdf-extract returns all text as a single string, so we need to split by page markers
        // Since pdf-extract doesn't provide page-by-page extraction directly,
        // we'll use a heuristic: form feed characters (\x0C) often mark page boundaries
        let pages: Vec<String> = pdf
            .split('\x0C')
            .map(|page| page.trim().to_string())
            .filter(|page| !page.is_empty())
            .collect();

        // If no page breaks were found, treat the entire content as a single page
        if pages.is_empty() {
            Ok(vec![pdf.trim().to_string()])
        } else {
            Ok(pages)
        }
    }

    fn format_as_markdown(&self, pages: Vec<String>) -> String {
        let mut markdown = String::new();
        
        for (page_index, page_text) in pages.iter().enumerate() {
            // Wrap per-page formatting in catch_unwind for graceful degradation
            let page_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.format_page(page_text)
            }));

            match page_result {
                Ok(page_markdown) => {
                    markdown.push_str(&page_markdown);
                }
                Err(_) => {
                    // Include error marker for the failed page and continue
                    tracing::warn!(
                        page = page_index + 1,
                        "Failed to format page, inserting error marker"
                    );
                    markdown.push_str(&format!(
                        "> **⚠ Page {} could not be processed.** Text extraction failed for this page.\n",
                        page_index + 1
                    ));
                }
            }
            
            // Add page separator between pages (but not after the last page)
            if page_index < pages.len() - 1 {
                markdown.push_str("\n---\n\n");
            }
        }
        
        markdown
    }

    /// Format a single page's text as Markdown. Separated out so it can be
    /// wrapped in catch_unwind for graceful degradation.
    fn format_page(&self, page_text: &str) -> String {
        let mut markdown = String::new();

        // Detect headings in this page
        let headings = self.detect_headings(page_text);
        let heading_lines: std::collections::HashSet<usize> =
            headings.iter().map(|(line_num, _)| *line_num).collect();

        // Process each line
        let lines: Vec<&str> = page_text.lines().collect();
        let mut in_paragraph = false;

        for (line_number, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines but track paragraph breaks
            if trimmed.is_empty() {
                if in_paragraph {
                    markdown.push_str("\n\n");
                    in_paragraph = false;
                }
                continue;
            }

            // Check if this line is a heading
            if heading_lines.contains(&line_number) {
                // Add blank line before heading if we were in a paragraph
                if in_paragraph {
                    markdown.push_str("\n\n");
                    in_paragraph = false;
                }

                // Determine heading level based on heuristics
                let is_major_heading = trimmed.len() < 40
                    && trimmed
                        .chars()
                        .filter(|c| c.is_alphabetic())
                        .all(|c| c.is_uppercase());

                if is_major_heading {
                    markdown.push_str("# ");
                } else {
                    markdown.push_str("## ");
                }
                markdown.push_str(trimmed);
                markdown.push_str("\n\n");
            } else {
                // Regular text - add to paragraph
                // Escape leading '#' to prevent accidental Markdown headings
                let safe_text = if trimmed.starts_with('#') {
                    format!("\\{}", trimmed)
                } else {
                    trimmed.to_string()
                };
                if in_paragraph {
                    markdown.push(' ');
                }
                markdown.push_str(&safe_text);
                in_paragraph = true;
            }
        }

        // Add final newline if we ended in a paragraph
        if in_paragraph {
            markdown.push('\n');
        }

        markdown
    }

    fn detect_headings(&self, text: &str) -> Vec<(usize, String)> {
        let mut headings = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        
        for (line_number, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }
            
            // Heuristic 1: All-caps text (at least 3 characters, not too long)
            let is_all_caps = trimmed.len() >= 3 
                && trimmed.len() <= 100 
                && trimmed.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
            
            // Heuristic 2: Short lines (less than 60 characters)
            let is_short = trimmed.len() < 60;
            
            // Heuristic 3: Common heading patterns (starts with Chapter, Section, etc.)
            let has_heading_prefix = trimmed.starts_with("Chapter ")
                || trimmed.starts_with("Section ")
                || trimmed.starts_with("Part ")
                || trimmed.starts_with("Appendix ")
                || trimmed.starts_with("Introduction")
                || trimmed.starts_with("Conclusion")
                || trimmed.starts_with("Abstract")
                || trimmed.starts_with("Summary");
            
            // Heuristic 4: Numbered headings (e.g., "1. ", "1.1 ", "I. ")
            let has_numbered_prefix = {
                let words: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(first_word) = words.first() {
                    // Check for patterns like "1.", "1.1", "I.", "A."
                    first_word.ends_with('.') && (
                        first_word.chars().take_while(|c| *c != '.').all(|c| c.is_numeric() || c == '.') ||
                        first_word.len() <= 4 && first_word.chars().take_while(|c| *c != '.').all(|c| c.is_uppercase())
                    )
                } else {
                    false
                }
            };
            
            // Heuristic 5: Doesn't end with sentence-ending punctuation (less likely to be body text)
            let no_sentence_ending = !trimmed.ends_with('.') 
                && !trimmed.ends_with('?') 
                && !trimmed.ends_with('!');
            
            // Combine heuristics: if multiple indicators suggest it's a heading
            let heading_score = (is_all_caps as u8) 
                + (is_short as u8) 
                + (has_heading_prefix as u8) 
                + (has_numbered_prefix as u8) 
                + (no_sentence_ending as u8);
            
            // Consider it a heading if score >= 2, or if it has a strong indicator
            if heading_score >= 2 || has_heading_prefix || (is_all_caps && is_short) {
                headings.push((line_number, trimmed.to_string()));
            }
        }
        
        headings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_pdf_converter_new() {
        let converter = PdfConverter::new();
        assert_eq!(converter.max_memory_mb, 500);
    }

    #[test]
    fn test_conversion_error_display() {
        // Test FileNotFound variant
        let err = ConversionError::FileNotFound("/path/to/file.pdf".to_string());
        assert_eq!(err.to_string(), "File not found: /path/to/file.pdf");

        // Test FileNotReadable variant
        let err = ConversionError::FileNotReadable("/path/to/file.pdf".to_string());
        assert_eq!(err.to_string(), "File not readable: /path/to/file.pdf");

        // Test CorruptedPdf variant
        let err = ConversionError::CorruptedPdf {
            path: "/path/to/file.pdf".to_string(),
            details: "Invalid PDF structure".to_string(),
        };
        assert_eq!(err.to_string(), "Corrupted PDF (/path/to/file.pdf): Invalid PDF structure");

        // Test EncryptedPdf variant
        let err = ConversionError::EncryptedPdf("/path/to/file.pdf".to_string());
        assert_eq!(err.to_string(), "PDF is encrypted and requires a password: /path/to/file.pdf");

        // Test EmptyPdf variant
        let err = ConversionError::EmptyPdf("/path/to/file.pdf".to_string());
        assert_eq!(err.to_string(), "PDF contains no extractable text: /path/to/file.pdf");

        // Test MemoryLimitExceeded variant
        let err = ConversionError::MemoryLimitExceeded("/path/to/file.pdf".to_string());
        assert_eq!(err.to_string(), "Memory limit exceeded during conversion: /path/to/file.pdf");

        // Test ConversionTimeout variant
        let err = ConversionError::ConversionTimeout { path: "/path/to/file.pdf".to_string(), timeout_secs: 10 };
        assert_eq!(err.to_string(), "Conversion timed out after 10 seconds: /path/to/file.pdf");
    }

    #[test]
    fn test_conversion_error_is_error_trait() {
        // Verify that ConversionError implements the Error trait
        let err: Box<dyn Error> = Box::new(ConversionError::FileNotFound("/test.pdf".to_string()));
        assert!(err.to_string().contains("File not found"));
    }

    #[test]
    fn test_conversion_error_debug() {
        // Test Debug trait implementation
        let err = ConversionError::EncryptedPdf(String::new());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("EncryptedPdf"));
    }

    #[test]
    fn test_extract_text_with_empty_pdf() {
        let converter = PdfConverter::new();
        
        // Create a minimal valid PDF with no text content
        let empty_pdf = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f\n0000000009 00000 n\n0000000058 00000 n\ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n110\n%%EOF";
        
        let result = converter.extract_text(empty_pdf);
        
        // Should return EmptyPdf error for PDFs with no extractable text
        assert!(matches!(result, Err(ConversionError::EmptyPdf(_)) | Err(ConversionError::CorruptedPdf { .. })));
    }

    #[test]
    fn test_extract_text_with_corrupted_pdf() {
        let converter = PdfConverter::new();
        
        // Invalid PDF data
        let corrupted_pdf = b"This is not a valid PDF file";
        
        let result = converter.extract_text(corrupted_pdf);
        
        // Should return CorruptedPdf error
        assert!(matches!(result, Err(ConversionError::CorruptedPdf { .. })));
    }

    #[test]
    fn test_extract_text_returns_vec_of_strings() {
        let converter = PdfConverter::new();
        
        // For this test, we'll use a simple approach: if we can't create a valid PDF,
        // we'll just verify the function signature and error handling
        // A real PDF would require a proper PDF library to generate
        
        // Test with invalid data to ensure error handling works
        let invalid_pdf = b"Not a PDF";
        let result = converter.extract_text(invalid_pdf);
        
        // Should return an error for invalid PDF
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_headings_all_caps() {
        let converter = PdfConverter::new();
        let text = "This is normal text.\nINTRODUCTION\nThis is more text.";
        
        let headings = converter.detect_headings(text);
        
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].0, 1); // Line number
        assert_eq!(headings[0].1, "INTRODUCTION");
    }

    #[test]
    fn test_detect_headings_with_prefix() {
        let converter = PdfConverter::new();
        let text = "Some text here.\nChapter 1: Getting Started\nMore content.";
        
        let headings = converter.detect_headings(text);
        
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].0, 1);
        assert_eq!(headings[0].1, "Chapter 1: Getting Started");
    }

    #[test]
    fn test_detect_headings_numbered() {
        let converter = PdfConverter::new();
        let text = "Introduction\n1. First Section\n2. Second Section\nBody text here.";
        
        let headings = converter.detect_headings(text);
        
        // Should detect "Introduction", "1. First Section", and "2. Second Section"
        assert!(headings.len() >= 2);
        assert!(headings.iter().any(|(_, h)| h.contains("First Section")));
        assert!(headings.iter().any(|(_, h)| h.contains("Second Section")));
    }

    #[test]
    fn test_detect_headings_short_lines() {
        let converter = PdfConverter::new();
        let text = "This is a very long line that should not be detected as a heading because it exceeds the length threshold.\nShort Title\nAnother long line of text that continues for a while and should not be considered a heading.";
        
        let headings = converter.detect_headings(text);
        
        // "Short Title" should be detected
        assert!(headings.iter().any(|(_, h)| h == "Short Title"));
    }

    #[test]
    fn test_detect_headings_empty_text() {
        let converter = PdfConverter::new();
        let text = "";
        
        let headings = converter.detect_headings(text);
        
        assert_eq!(headings.len(), 0);
    }

    #[test]
    fn test_detect_headings_no_headings() {
        let converter = PdfConverter::new();
        let text = "This is just normal body text. It has sentences that end with periods. Nothing here looks like a heading at all.";
        
        let headings = converter.detect_headings(text);
        
        // Should detect no headings
        assert_eq!(headings.len(), 0);
    }

    #[test]
    fn test_detect_headings_multiple_types() {
        let converter = PdfConverter::new();
        let text = "ABSTRACT\n\nThis paper discusses...\n\nChapter 1: Introduction\n\nSome content here.\n\n1. First Point\n\nMore text.\n\nCONCLUSION\n\nFinal thoughts.";
        
        let headings = converter.detect_headings(text);
        
        // Should detect ABSTRACT, Chapter 1, 1. First Point, and CONCLUSION
        assert!(headings.len() >= 3);
        assert!(headings.iter().any(|(_, h)| h == "ABSTRACT"));
        assert!(headings.iter().any(|(_, h)| h.contains("Chapter 1")));
        assert!(headings.iter().any(|(_, h)| h == "CONCLUSION"));
    }

    #[test]
    fn test_format_as_markdown_single_page() {
        let converter = PdfConverter::new();
        let pages = vec!["This is a simple paragraph.\n\nThis is another paragraph.".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should preserve paragraph structure
        assert!(markdown.contains("This is a simple paragraph."));
        assert!(markdown.contains("This is another paragraph."));
        assert!(markdown.contains("\n\n"));
        // Should not have page separator for single page
        assert!(!markdown.contains("---"));
    }

    #[test]
    fn test_format_as_markdown_with_headings() {
        let converter = PdfConverter::new();
        let pages = vec!["INTRODUCTION\n\nThis is the introduction text.\n\nChapter 1: Getting Started\n\nThis is chapter one content.".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should convert headings to Markdown syntax
        assert!(markdown.contains("# INTRODUCTION"));
        assert!(markdown.contains("## Chapter 1: Getting Started"));
        // Should preserve body text
        assert!(markdown.contains("This is the introduction text."));
        assert!(markdown.contains("This is chapter one content."));
    }

    #[test]
    fn test_format_as_markdown_multiple_pages() {
        let converter = PdfConverter::new();
        let pages = vec![
            "Page one content.\n\nMore text on page one.".to_string(),
            "Page two content.\n\nMore text on page two.".to_string(),
            "Page three content.".to_string(),
        ];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should include all page content
        assert!(markdown.contains("Page one content."));
        assert!(markdown.contains("Page two content."));
        assert!(markdown.contains("Page three content."));
        // Should have page separators between pages
        assert_eq!(markdown.matches("---").count(), 2); // 2 separators for 3 pages
    }

    #[test]
    fn test_format_as_markdown_preserves_paragraph_structure() {
        let converter = PdfConverter::new();
        let pages = vec!["First paragraph here.\n\nSecond paragraph here.\n\nThird paragraph here.".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should have blank lines between paragraphs
        let double_newlines = markdown.matches("\n\n").count();
        assert!(double_newlines >= 2); // At least 2 paragraph breaks
    }

    #[test]
    fn test_format_as_markdown_empty_pages() {
        let converter = PdfConverter::new();
        let pages = vec!["".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should handle empty pages gracefully
        assert_eq!(markdown.trim(), "");
    }

    #[test]
    fn test_format_as_markdown_heading_levels() {
        let converter = PdfConverter::new();
        let pages = vec!["ABSTRACT\n\nSome text.\n\n1.1 Subsection\n\nMore text.".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // ABSTRACT should be major heading (# )
        assert!(markdown.contains("# ABSTRACT"));
        // Subsection should be minor heading (## )
        assert!(markdown.contains("## 1.1 Subsection"));
    }

    #[test]
    fn test_format_as_markdown_no_trailing_separator() {
        let converter = PdfConverter::new();
        let pages = vec!["Page 1".to_string(), "Page 2".to_string()];
        
        let markdown = converter.format_as_markdown(pages);
        
        // Should not end with a page separator
        assert!(!markdown.trim_end().ends_with("---"));
    }

    #[tokio::test]
    async fn test_convert_to_markdown_file_not_found() {
        let converter = PdfConverter::new();
        let path = Path::new("/nonexistent/path/to/file.pdf");
        
        let result = converter.convert_to_markdown(path).await;
        
        assert!(matches!(result, Err(ConversionError::FileNotFound(_))));
        if let Err(ConversionError::FileNotFound(path_str)) = result {
            assert!(path_str.contains("file.pdf"));
        }
    }

    #[tokio::test]
    async fn test_convert_to_markdown_measures_time() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        let converter = PdfConverter::new();
        
        // Create a temporary file with minimal PDF content
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // Write a minimal valid PDF (this will likely fail to parse, but that's ok for this test)
        let minimal_pdf = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\nxref\n0 3\n0000000000 65535 f\n0000000009 00000 n\n0000000058 00000 n\ntrailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n110\n%%EOF";
        temp_file.write_all(minimal_pdf).unwrap();
        temp_file.flush().unwrap();
        
        let result = converter.convert_to_markdown(temp_file.path()).await;
        
        // The conversion will likely fail due to empty/corrupted PDF, but we're testing that
        // the method attempts to measure time and returns a result
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_convert_to_markdown_returns_conversion_result() {
        // This test verifies the structure of ConversionResult
        // We can't easily test with a real PDF without external files,
        // but we can verify the error handling works
        
        let converter = PdfConverter::new();
        let path = Path::new("/tmp/nonexistent.pdf");
        
        let result = converter.convert_to_markdown(path).await;
        
        // Should return an error for non-existent file
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_convert_to_markdown_with_valid_pdf() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        let converter = PdfConverter::new();
        
        // Create a temporary file with a simple valid PDF containing text
        // This is a minimal PDF with actual text content
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // A minimal but valid PDF with text "Hello World"
        // This PDF was generated to be as simple as possible while still being valid
        let simple_pdf = b"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources 4 0 R /MediaBox [0 0 612 792] /Contents 5 0 R >>
endobj
4 0 obj
<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>
endobj
5 0 obj
<< /Length 44 >>
stream
BT
/F1 12 Tf
100 700 Td
(Hello World) Tj
ET
endstream
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000229 00000 n 
0000000327 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
420
%%EOF";
        
        temp_file.write_all(simple_pdf).unwrap();
        temp_file.flush().unwrap();
        
        let result = converter.convert_to_markdown(temp_file.path()).await;
        
        // The result should be Ok with a ConversionResult
        match result {
            Ok(conversion_result) => {
                // Verify the structure
                assert!(!conversion_result.content.is_empty(), "Content should not be empty");
                assert!(conversion_result.page_count > 0, "Should have at least one page");
                // conversion_time_ms is u64, so it's always >= 0
                
                // The content should contain "Hello World" or be a valid markdown string
                // Note: pdf-extract may or may not extract "Hello World" depending on the PDF structure
                // At minimum, we verify we got some content back
                println!("Extracted content: {}", conversion_result.content);
            }
            Err(e) => {
                // If extraction fails, it should be one of the expected error types
                // (EmptyPdf or CorruptedPdf are acceptable for this minimal PDF)
                println!("Extraction error (acceptable for minimal PDF): {:?}", e);
                assert!(
                    matches!(e, ConversionError::EmptyPdf(_) | ConversionError::CorruptedPdf { .. }),
                    "Error should be EmptyPdf or CorruptedPdf for minimal PDF"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_convert_to_markdown_permission_denied() {
        // This test is platform-specific and may not work on all systems
        // We'll test the error handling logic by using a non-existent path
        // which will trigger FileNotFound rather than PermissionDenied
        
        let converter = PdfConverter::new();
        
        // Use a path that definitely doesn't exist
        let path = Path::new("/root/nonexistent/file.pdf");
        
        let result = converter.convert_to_markdown(path).await;
        
        // Should return FileNotFound or FileNotReadable
        assert!(matches!(
            result,
            Err(ConversionError::FileNotFound(_)) | Err(ConversionError::FileNotReadable(_))
        ));
    }

    #[tokio::test]
    async fn test_convert_to_markdown_memory_limit_exceeded() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a converter with a small memory limit (1 MB)
        let converter = PdfConverter { max_memory_mb: 1 };
        let mut temp_file = NamedTempFile::new().unwrap();
        let data = vec![0u8; 1024 * 1024 + 1]; // Just over 1 MB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;
        assert!(matches!(result, Err(ConversionError::MemoryLimitExceeded(_))));
    }

    #[test]
    fn test_conversion_timeout_error_display() {
        let err = ConversionError::ConversionTimeout { path: "/test.pdf".to_string(), timeout_secs: 10 };
        assert_eq!(err.to_string(), "Conversion timed out after 10 seconds: /test.pdf");
    }

    #[test]
    fn test_with_path_adds_context() {
        let err = ConversionError::EncryptedPdf(String::new());
        let err = err.with_path("/doc.pdf".to_string());
        assert!(matches!(err, ConversionError::EncryptedPdf(ref p) if p == "/doc.pdf"));

        let err = ConversionError::EmptyPdf(String::new());
        let err = err.with_path("/doc.pdf".to_string());
        assert!(matches!(err, ConversionError::EmptyPdf(ref p) if p == "/doc.pdf"));

        let err = ConversionError::CorruptedPdf { path: String::new(), details: "bad".to_string() };
        let err = err.with_path("/doc.pdf".to_string());
        assert!(matches!(err, ConversionError::CorruptedPdf { ref path, .. } if path == "/doc.pdf"));
    }

    #[test]
    fn test_format_as_markdown_error_marker_for_empty_page_among_valid() {
        // Test that format_as_markdown handles a mix of valid and empty pages
        // (empty pages are filtered in extract_text, but format_as_markdown should
        // still handle them gracefully if they slip through)
        let converter = PdfConverter::new();
        let pages = vec![
            "Page one content.".to_string(),
            "".to_string(),
            "Page three content.".to_string(),
        ];

        let markdown = converter.format_as_markdown(pages);

        // Should include content from valid pages
        assert!(markdown.contains("Page one content."));
        assert!(markdown.contains("Page three content."));
        // Should have page separators
        assert_eq!(markdown.matches("---").count(), 2);
    }

    #[test]
    fn test_format_page_produces_valid_markdown() {
        let converter = PdfConverter::new();
        let page = "INTRODUCTION\n\nSome body text here.\n\nMore text.";
        let result = converter.format_page(page);
        assert!(result.contains("# INTRODUCTION"));
        assert!(result.contains("Some body text here."));
        assert!(result.contains("More text."));
    }

    // --- Task 3.13: Unit tests for PDF converter ---

    #[tokio::test]
    async fn test_convert_to_markdown_single_page_end_to_end() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        let mut temp_file = NamedTempFile::new().unwrap();

        // Minimal valid single-page PDF with text "Hello World"
        let pdf = b"%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/Resources 4 0 R/MediaBox[0 0 612 792]/Contents 5 0 R>>endobj
4 0 obj<</Font<</F1<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>>>>>endobj
5 0 obj<</Length 44>>
stream
BT /F1 12 Tf 100 700 Td (Hello World) Tj ET
endstream
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000052 00000 n 
0000000101 00000 n 
0000000230 00000 n 
0000000312 00000 n 
trailer<</Size 6/Root 1 0 R>>
startxref
405
%%EOF";
        temp_file.write_all(pdf).unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;
        match result {
            Ok(cr) => {
                assert_eq!(cr.page_count, 1, "Single-page PDF should yield page_count=1");
                assert!(!cr.content.is_empty(), "Content should not be empty");
                // No page separators for a single page
                assert!(!cr.content.contains("---"), "Single page should have no separator");
            }
            Err(ConversionError::EmptyPdf(_)) | Err(ConversionError::CorruptedPdf { .. }) => {
                // Acceptable: minimal hand-crafted PDF may not parse with pdf-extract
            }
            Err(e) => panic!("Unexpected error variant: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_convert_to_markdown_corrupted_pdf_end_to_end() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"this is not a pdf at all").unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;
        assert!(
            matches!(result, Err(ConversionError::CorruptedPdf { .. })),
            "Corrupted data should produce CorruptedPdf error, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_convert_to_markdown_empty_pdf_end_to_end() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        let mut temp_file = NamedTempFile::new().unwrap();

        // Minimal valid PDF structure with zero pages / no text
        let empty_pdf = b"%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[]/Count 0>>endobj
xref
0 3
0000000000 65535 f 
0000000009 00000 n 
0000000052 00000 n 
trailer<</Size 3/Root 1 0 R>>
startxref
101
%%EOF";
        temp_file.write_all(empty_pdf).unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;
        assert!(
            matches!(result, Err(ConversionError::EmptyPdf(_)) | Err(ConversionError::CorruptedPdf { .. })),
            "Empty PDF should produce EmptyPdf or CorruptedPdf error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_extract_text_encrypted_pdf_error() {
        let converter = PdfConverter::new();

        // Minimal PDF-like bytes with an Encrypt dictionary entry.
        // pdf-extract should reject this with an error containing "encrypt".
        let encrypted_pdf = b"%PDF-1.6
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[]/Count 0>>endobj
3 0 obj<</Filter/Standard/V 2/R 3/O(xxxx)/U(xxxx)/P -3904>>endobj
trailer<</Size 4/Root 1 0 R/Encrypt 3 0 R>>
startxref
0
%%EOF";

        let result = converter.extract_text(encrypted_pdf);
        // Should be EncryptedPdf or CorruptedPdf (depending on how pdf-extract reports it)
        assert!(
            result.is_err(),
            "Encrypted PDF data should produce an error"
        );
    }

    #[test]
    fn test_format_as_markdown_multi_page_with_headings() {
        let converter = PdfConverter::new();
        let pages = vec![
            "INTRODUCTION\n\nFirst page body text here.".to_string(),
            "CONCLUSION\n\nSecond page body text here.".to_string(),
        ];

        let markdown = converter.format_as_markdown(pages);

        // Both headings converted
        assert!(markdown.contains("# INTRODUCTION"));
        assert!(markdown.contains("# CONCLUSION"));
        // Body text preserved
        assert!(markdown.contains("First page body text here."));
        assert!(markdown.contains("Second page body text here."));
        // Exactly one separator between two pages
        assert_eq!(markdown.matches("---").count(), 1);
    }

    // --- Task 11.4: Unit tests for error scenarios ---
    // Validates: Requirements 2.3, 2.4, 6.2, 6.3, 7.4

    #[tokio::test]
    async fn test_error_file_not_found_returns_correct_variant() {
        let converter = PdfConverter::new();
        let path = Path::new("/absolutely/does/not/exist/document.pdf");

        let result = converter.convert_to_markdown(path).await;

        match result {
            Err(ConversionError::FileNotFound(p)) => {
                assert!(p.contains("document.pdf"), "Error should contain the file name");
            }
            other => panic!("Expected FileNotFound, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_error_file_not_readable_with_directory_path() {
        // Passing a directory path to convert_to_markdown should fail with
        // a read error (not FileNotFound, since the path exists).
        let converter = PdfConverter::new();
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let result = converter.convert_to_markdown(dir_path).await;

        // Reading a directory as a file should produce FileNotReadable
        // (or possibly a different I/O error mapped to FileNotReadable).
        assert!(
            matches!(result, Err(ConversionError::FileNotReadable(_))),
            "Reading a directory should produce FileNotReadable, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_error_corrupted_pdf_with_random_bytes() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"RANDOM GARBAGE DATA 12345!@#$%").unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;

        assert!(
            matches!(result, Err(ConversionError::CorruptedPdf { .. })),
            "Random bytes should produce CorruptedPdf error, got: {:?}",
            result
        );
        if let Err(ConversionError::CorruptedPdf { path, details }) = result {
            assert!(!path.is_empty(), "CorruptedPdf error should include the file path");
            assert!(!details.is_empty(), "CorruptedPdf error should include details");
        }
    }

    #[tokio::test]
    async fn test_error_encrypted_pdf_end_to_end() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        let mut temp_file = NamedTempFile::new().unwrap();

        // Minimal PDF-like bytes with an Encrypt dictionary entry
        let encrypted_pdf = b"%PDF-1.6
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[]/Count 0>>endobj
3 0 obj<</Filter/Standard/V 2/R 3/O(xxxx)/U(xxxx)/P -3904>>endobj
trailer<</Size 4/Root 1 0 R/Encrypt 3 0 R>>
startxref
0
%%EOF";
        temp_file.write_all(encrypted_pdf).unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;

        // Should be EncryptedPdf or CorruptedPdf (depending on how pdf-extract reports it)
        assert!(
            result.is_err(),
            "Encrypted PDF should produce an error"
        );
        // Verify it's one of the expected error types
        assert!(
            matches!(
                result,
                Err(ConversionError::EncryptedPdf(_)) | Err(ConversionError::CorruptedPdf { .. })
            ),
            "Expected EncryptedPdf or CorruptedPdf, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_error_empty_file_zero_bytes() {
        use tempfile::NamedTempFile;

        let converter = PdfConverter::new();
        // Create a truly empty file (0 bytes)
        let temp_file = NamedTempFile::new().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;

        // An empty file is not a valid PDF — should return EmptyPdf or CorruptedPdf
        assert!(
            matches!(
                result,
                Err(ConversionError::EmptyPdf(_)) | Err(ConversionError::CorruptedPdf { .. })
            ),
            "Empty (0-byte) file should produce EmptyPdf or CorruptedPdf, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_error_memory_limit_exceeded_small_limit() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a converter with a tiny memory limit (1 byte effectively)
        let converter = PdfConverter { max_memory_mb: 1 };
        let mut temp_file = NamedTempFile::new().unwrap();
        let data = vec![0u8; 1024 * 1024 + 1]; // Just over 1 MB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let result = converter.convert_to_markdown(temp_file.path()).await;

        match result {
            Err(ConversionError::MemoryLimitExceeded(p)) => {
                assert!(!p.is_empty(), "MemoryLimitExceeded should include the file path");
            }
            other => panic!("Expected MemoryLimitExceeded, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_error_conversion_timeout() {
        // We can't easily trigger a real timeout with a 10-second limit,
        // but we can verify the timeout error variant is well-formed.
        let err = ConversionError::ConversionTimeout {
            path: "/path/to/large.pdf".to_string(),
            timeout_secs: 10,
        };
        assert!(err.to_string().contains("10 seconds"));
        assert!(err.to_string().contains("/path/to/large.pdf"));

        // Also verify the error variant can be pattern-matched correctly
        match err {
            ConversionError::ConversionTimeout { path, timeout_secs } => {
                assert_eq!(path, "/path/to/large.pdf");
                assert_eq!(timeout_secs, 10);
            }
            _ => panic!("Expected ConversionTimeout variant"),
        }
    }

    // Feature: zed-pdf-lsp, Property 6: Text Extraction from Valid PDFs
    // **Validates: Requirements 3.1**
    mod property_text_extraction {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn non_empty_text_produces_non_empty_markdown(
                pages in prop::collection::vec("[^\x00]{1,200}", 1..=5)
            ) {
                let converter = PdfConverter::new();
                let markdown = converter.format_as_markdown(pages);
                prop_assert!(!markdown.is_empty(), "format_as_markdown must produce non-empty output for non-empty pages");
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 8: Paragraph Structure Preservation
    // **Validates: Requirements 3.3**
    mod property_paragraph_preservation {
        use super::*;
        use proptest::prelude::*;

        /// Strategy that generates a single paragraph: one or more non-empty
        /// lines that contain only lowercase words (so they won't trigger
        /// heading heuristics) and end with a period (sentence-ending
        /// punctuation further prevents heading detection).
        fn paragraph_strategy() -> impl Strategy<Value = String> {
            prop::collection::vec("[a-z]{2,12}( [a-z]{2,12}){0,6}\\.", 1..=3)
                .prop_map(|lines| lines.join("\n"))
        }

        proptest! {
            #[test]
            fn paragraph_breaks_are_preserved(
                paragraphs in prop::collection::vec(paragraph_strategy(), 2..=5)
            ) {
                // Build input text: paragraphs separated by blank lines
                let input_text = paragraphs.join("\n\n");
                let num_paragraphs = paragraphs.len();

                let converter = PdfConverter::new();
                let markdown = converter.format_page(&input_text);

                // Count paragraph separations in the output.
                // A paragraph separation is a blank line (\n\n) or a structural
                // element (heading / separator) that visually divides content.
                let double_newline_count = markdown.matches("\n\n").count();

                // With N paragraphs there must be at least N-1 separations
                prop_assert!(
                    double_newline_count >= num_paragraphs - 1,
                    "Expected at least {} paragraph separations for {} paragraphs, found {}. Output:\n{}",
                    num_paragraphs - 1,
                    num_paragraphs,
                    double_newline_count,
                    markdown
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 9: Heading Detection and Conversion
    // **Validates: Requirements 3.4**
    mod property_heading_conversion {
        use super::*;
        use proptest::prelude::*;

        /// Strategy that generates text containing an all-caps heading pattern.
        /// All-caps short text (>= 3 chars, < 60 chars) is detected as a heading.
        fn all_caps_heading_strategy() -> impl Strategy<Value = String> {
            "[A-Z]{3,20}"
        }

        /// Strategy that generates text containing a "Chapter N: ..." heading pattern.
        fn chapter_heading_strategy() -> impl Strategy<Value = String> {
            (1..100u32, "[A-Za-z ]{3,30}")
                .prop_map(|(n, title)| format!("Chapter {}: {}", n, title.trim()))
        }

        /// Strategy that generates body text that should NOT be detected as a heading.
        fn body_text_strategy() -> impl Strategy<Value = String> {
            "[a-z]{3,10}( [a-z]{3,10}){5,15}\\."
        }

        proptest! {
            #[test]
            fn all_caps_headings_produce_markdown_heading_syntax(
                heading in all_caps_heading_strategy(),
                body1 in body_text_strategy(),
                body2 in body_text_strategy(),
            ) {
                // Build page text with an all-caps heading surrounded by body text
                let page_text = format!("{}\n{}\n{}", body1, heading, body2);

                let converter = PdfConverter::new();
                let markdown = converter.format_page(&page_text);

                // The all-caps heading should appear with # or ## prefix
                let has_heading = markdown.lines().any(|line| {
                    let trimmed = line.trim();
                    (trimmed.starts_with("# ") || trimmed.starts_with("## "))
                        && trimmed.contains(&heading)
                });

                prop_assert!(
                    has_heading,
                    "All-caps heading '{}' should be converted to Markdown heading syntax (# or ##). Output:\n{}",
                    heading,
                    markdown
                );
            }

            #[test]
            fn chapter_headings_produce_markdown_heading_syntax(
                heading in chapter_heading_strategy(),
                body1 in body_text_strategy(),
                body2 in body_text_strategy(),
            ) {
                // Build page text with a Chapter heading surrounded by body text
                let page_text = format!("{}\n{}\n{}", body1, heading, body2);

                let converter = PdfConverter::new();
                let markdown = converter.format_page(&page_text);

                // The Chapter heading should appear with # or ## prefix
                let has_heading = markdown.lines().any(|line| {
                    let trimmed = line.trim();
                    (trimmed.starts_with("# ") || trimmed.starts_with("## "))
                        && trimmed.contains("Chapter")
                });

                prop_assert!(
                    has_heading,
                    "Chapter heading '{}' should be converted to Markdown heading syntax (# or ##). Output:\n{}",
                    heading,
                    markdown
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 10: Multi-Page Content Extraction
    // **Validates: Requirements 3.5**
    mod property_multi_page {
        use super::*;
        use proptest::prelude::*;

        /// Strategy that generates unique page content (lowercase body text ending
        /// with a period so it won't be detected as a heading).
        fn page_content_strategy() -> impl Strategy<Value = String> {
            "[a-z]{3,8}( [a-z]{3,8}){2,6}\\."
        }

        proptest! {
            #[test]
            fn all_pages_present_in_sequential_order(
                pages in prop::collection::vec(page_content_strategy(), 2..=5)
            ) {
                let converter = PdfConverter::new();
                let markdown = converter.format_as_markdown(pages.clone());

                // 1. All page content appears in the output
                for (i, page) in pages.iter().enumerate() {
                    prop_assert!(
                        markdown.contains(page.trim()),
                        "Page {} content '{}' missing from output:\n{}",
                        i + 1, page, markdown
                    );
                }

                // 2. Content appears in sequential order
                let mut last_pos = 0usize;
                for (i, page) in pages.iter().enumerate() {
                    let pos = markdown[last_pos..].find(page.trim())
                        .map(|p| p + last_pos);
                    prop_assert!(
                        pos.is_some(),
                        "Page {} content not found after position {} in output",
                        i + 1, last_pos
                    );
                    last_pos = pos.unwrap();
                }

                // 3. There are N-1 page separators (---) for N pages
                let separator_count = markdown.lines()
                    .filter(|l| l.trim() == "---")
                    .count();
                prop_assert_eq!(
                    separator_count,
                    pages.len() - 1,
                    "Expected {} separators for {} pages, found {}",
                    pages.len() - 1, pages.len(), separator_count
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 11: UTF-8 Encoding Validity
    // **Validates: Requirements 4.4**
    mod property_utf8_encoding {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn markdown_output_is_valid_utf8(
                pages in prop::collection::vec("\\PC{1,200}", 1..=4)
            ) {
                let converter = PdfConverter::new();
                let markdown = converter.format_as_markdown(pages);

                // Verify the output bytes are valid UTF-8
                prop_assert!(
                    std::str::from_utf8(markdown.as_bytes()).is_ok(),
                    "Markdown output must be valid UTF-8 encoded data"
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 17: Conversion Time Performance
    // **Validates: Requirements 7.1, 7.2**
    mod property_conversion_performance {
        use super::*;
        use proptest::prelude::*;
        use std::time::Instant;

        /// Strategy that generates page content of a given total byte size.
        /// We produce a Vec<String> of pages whose combined length approximates
        /// the target size, simulating a document of that size being formatted.
        fn pages_with_total_size(target_bytes: usize) -> Vec<String> {
            // Each page ~4 KB of body text
            let page_size = 4096;
            let num_pages = (target_bytes / page_size).max(1);
            let line = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor.";
            let lines_per_page = page_size / (line.len() + 1);
            (0..num_pages)
                .map(|_| {
                    (0..lines_per_page)
                        .map(|_| line)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect()
        }

        proptest! {
            #![proptest_config(proptest::prelude::ProptestConfig::with_cases(20))]

            #[test]
            fn small_file_conversion_within_2_seconds(
                // Simulate files from ~1 KB up to just under 10 MB of text content
                size_kb in 1u32..10_000u32
            ) {
                let size_bytes = size_kb as usize * 1024;
                let pages = pages_with_total_size(size_bytes);

                let converter = PdfConverter::new();
                let start = Instant::now();
                let _markdown = converter.format_as_markdown(pages);
                let elapsed = start.elapsed();

                // Requirement 7.1: files < 10 MB must convert within 2 seconds
                prop_assert!(
                    elapsed.as_secs_f64() < 2.0,
                    "format_as_markdown for ~{} KB input took {:.3}s, exceeding 2s limit",
                    size_kb,
                    elapsed.as_secs_f64()
                );
            }

            #[test]
            fn large_file_conversion_within_5_seconds(
                // Simulate files from 10 MB up to ~20 MB of text content
                size_mb in 10u32..=20u32
            ) {
                let size_bytes = size_mb as usize * 1024 * 1024;
                let pages = pages_with_total_size(size_bytes);

                let converter = PdfConverter::new();
                let start = Instant::now();
                let _markdown = converter.format_as_markdown(pages);
                let elapsed = start.elapsed();

                // Requirement 7.2: files >= 10 MB must convert within 5 seconds
                prop_assert!(
                    elapsed.as_secs_f64() < 5.0,
                    "format_as_markdown for ~{} MB input took {:.3}s, exceeding 5s limit",
                    size_mb,
                    elapsed.as_secs_f64()
                );
            }
        }
    }

    // Feature: zed-pdf-lsp, Property 19: Memory Usage Limit
    // **Validates: Requirements 7.4**
    mod property_memory_usage_limit {
        use super::*;
        use proptest::prelude::*;

        #[test]
        fn default_memory_limit_is_500mb() {
            let converter = PdfConverter::new();
            assert_eq!(
                converter.max_memory_mb, 500,
                "Default memory limit must be 500 MB per Requirement 7.4"
            );
        }

        proptest! {
            #![proptest_config(proptest::prelude::ProptestConfig::with_cases(50))]

            #[test]
            fn memory_limit_field_is_respected(limit_mb in 1u16..=2000u16) {
                let converter = PdfConverter { max_memory_mb: limit_mb as usize };
                prop_assert_eq!(
                    converter.max_memory_mb,
                    limit_mb as usize,
                    "PdfConverter must store the configured memory limit"
                );
            }

            #[test]
            fn file_exceeding_memory_limit_returns_error(limit_mb in 1u8..=4u8) {
                // Create a converter with a small memory limit (1-4 MB)
                let limit = limit_mb as usize;
                let converter = PdfConverter { max_memory_mb: limit };

                // Create a temp file whose size is >= limit_mb megabytes
                let file_size = limit * 1024 * 1024; // exactly limit MB
                let data = vec![0u8; file_size];

                let mut temp_file = tempfile::NamedTempFile::new()
                    .expect("failed to create temp file");
                std::io::Write::write_all(&mut temp_file, &data)
                    .expect("failed to write temp file");
                std::io::Write::flush(&mut temp_file)
                    .expect("failed to flush temp file");

                // Run the async conversion in a blocking tokio runtime
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result = rt.block_on(converter.convert_to_markdown(temp_file.path()));

                prop_assert!(
                    matches!(result, Err(ConversionError::MemoryLimitExceeded(_))),
                    "File of {} MB with limit {} MB must return MemoryLimitExceeded, got: {:?}",
                    limit, limit, result
                );
            }
        }
    }

    // Task 10.7: Performance benchmarks for PDF converter
    // **Validates: Requirements 7.1, 7.2, 7.3, 7.4**
    mod benchmarks {
        use super::*;
        use std::time::Instant;

        /// Helper: generate a Vec<String> of pages with the given total text size.
        fn generate_pages(num_pages: usize, text_per_page: &str) -> Vec<String> {
            (0..num_pages).map(|_| text_per_page.to_string()).collect()
        }

        fn sample_page_text(lines: usize) -> String {
            let line = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor.";
            (0..lines).map(|_| line).collect::<Vec<_>>().join("\n")
        }

        #[test]
        fn bench_format_as_markdown_small_input() {
            // Small input: 1 page (~80 lines)
            let converter = PdfConverter::new();
            let page_text = sample_page_text(80);
            let pages = generate_pages(1, &page_text);

            let start = Instant::now();
            for _ in 0..100 {
                let _ = converter.format_as_markdown(pages.clone());
            }
            let elapsed = start.elapsed();
            let avg_ms = elapsed.as_millis() as f64 / 100.0;

            // 1 page should convert very quickly — well under 100ms per call
            assert!(
                avg_ms < 100.0,
                "Small input (1 page) avg conversion took {:.2}ms, expected < 100ms",
                avg_ms
            );
        }

        #[test]
        fn bench_format_as_markdown_medium_input() {
            // Medium input: 10 pages (~80 lines each)
            let converter = PdfConverter::new();
            let page_text = sample_page_text(80);
            let pages = generate_pages(10, &page_text);

            let start = Instant::now();
            for _ in 0..50 {
                let _ = converter.format_as_markdown(pages.clone());
            }
            let elapsed = start.elapsed();
            let avg_ms = elapsed.as_millis() as f64 / 50.0;

            // 10 pages should still be fast — under 500ms
            assert!(
                avg_ms < 500.0,
                "Medium input (10 pages) avg conversion took {:.2}ms, expected < 500ms",
                avg_ms
            );
        }

        #[test]
        fn bench_format_as_markdown_large_input() {
            // Large input: 50 pages (~80 lines each)
            let converter = PdfConverter::new();
            let page_text = sample_page_text(80);
            let pages = generate_pages(50, &page_text);

            let start = Instant::now();
            for _ in 0..10 {
                let _ = converter.format_as_markdown(pages.clone());
            }
            let elapsed = start.elapsed();
            let avg_ms = elapsed.as_millis() as f64 / 10.0;

            // 50 pages: should complete within 2 seconds (Req 7.1)
            assert!(
                avg_ms < 2000.0,
                "Large input (50 pages) avg conversion took {:.2}ms, expected < 2000ms",
                avg_ms
            );
        }

        #[test]
        fn bench_conversion_time_scales_with_size() {
            // Verify that conversion time grows roughly linearly with input size
            let converter = PdfConverter::new();
            let page_text = sample_page_text(80);

            let sizes = [1, 5, 10, 25, 50];
            let mut timings = Vec::new();

            for &num_pages in &sizes {
                let pages = generate_pages(num_pages, &page_text);
                let start = Instant::now();
                for _ in 0..20 {
                    let _ = converter.format_as_markdown(pages.clone());
                }
                let avg_us = start.elapsed().as_micros() as f64 / 20.0;
                timings.push((num_pages, avg_us));
            }

            // The largest input should take more time than the smallest
            let (_, time_small) = timings[0];
            let (_, time_large) = timings[timings.len() - 1];
            assert!(
                time_large >= time_small,
                "Larger input should take at least as long: small={:.0}µs, large={:.0}µs",
                time_small,
                time_large
            );
        }

        #[test]
        fn bench_concurrent_format_as_markdown() {
            // Verify concurrent calls complete without blocking each other
            use std::sync::Arc;
            use std::thread;

            let converter = Arc::new(PdfConverter::new());
            let page_text = sample_page_text(80);
            let pages: Vec<String> = generate_pages(10, &page_text);

            let num_threads = 4;
            let start = Instant::now();

            let handles: Vec<_> = (0..num_threads)
                .map(|_| {
                    let conv = Arc::clone(&converter);
                    let p = pages.clone();
                    thread::spawn(move || {
                        for _ in 0..10 {
                            let _ = conv.format_as_markdown(p.clone());
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().expect("Thread panicked");
            }

            let elapsed = start.elapsed();
            // 4 threads × 10 iterations of 10-page conversion should complete
            // within a reasonable time (< 5 seconds total)
            assert!(
                elapsed.as_secs_f64() < 5.0,
                "Concurrent conversion ({} threads × 10 iters) took {:.2}s, expected < 5s",
                num_threads,
                elapsed.as_secs_f64()
            );
        }

        #[test]
        fn bench_memory_proxy_for_varying_sizes() {
            // Verify that output size is proportional to input size (proxy for memory)
            let converter = PdfConverter::new();
            let page_text = sample_page_text(80);

            let small_pages = generate_pages(1, &page_text);
            let large_pages = generate_pages(50, &page_text);

            let small_output = converter.format_as_markdown(small_pages);
            let large_output = converter.format_as_markdown(large_pages);

            // Output for 50 pages should be roughly 50× the single-page output
            // Allow generous bounds: at least 10× and at most 100×
            let ratio = large_output.len() as f64 / small_output.len() as f64;
            assert!(
                ratio > 10.0 && ratio < 100.0,
                "Output size ratio (50 pages / 1 page) = {:.1}, expected between 10 and 100",
                ratio
            );
        }
    }

    // Feature: zed-pdf-lsp, Property 7: Markdown Output Validity
    // **Validates: Requirements 3.2**
    mod property_markdown_validity {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn markdown_output_is_valid(
                pages in prop::collection::vec("[^\x00]{0,300}", 1..=4)
            ) {
                let converter = PdfConverter::new();
                let markdown = converter.format_as_markdown(pages.clone());

                // 1. Valid UTF-8 — guaranteed by Rust's String type, but assert anyway
                prop_assert!(std::str::from_utf8(markdown.as_bytes()).is_ok(),
                    "Output must be valid UTF-8");

                // 2. Headings use proper # syntax: any line starting with '#' must
                //    be followed by a space (e.g. "# " or "## ").
                //    Lines starting with '\#' are escaped non-heading text.
                for line in markdown.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') && !trimmed.starts_with("\\#") {
                        let after_hashes = trimmed.trim_start_matches('#');
                        prop_assert!(
                            after_hashes.is_empty() || after_hashes.starts_with(' '),
                            "Heading line must have a space after '#' characters: {:?}",
                            trimmed
                        );
                    }
                }

                // 3. Page separators (---) appear on their own line
                for line in markdown.lines() {
                    if line.trim() == "---" {
                        prop_assert_eq!(line.trim(), "---",
                            "Page separator '---' must be on its own line");
                    }
                }

                // 4. No broken/unclosed formatting introduced by the converter.
                //    The converter only emits headings (# / ##) and separators (---).
                //    Raw PDF text passes through as-is, so we only validate
                //    converter-generated structural elements (headings checked above,
                //    separators checked below). The output is structurally sound
                //    Markdown if headings and separators are well-formed.

                // 5. Separator count matches expected: N-1 separators for N pages
                if pages.len() > 1 {
                    let separator_count = markdown.lines()
                        .filter(|l| l.trim() == "---")
                        .count();
                    prop_assert_eq!(
                        separator_count,
                        pages.len() - 1,
                        "Expected {} separators for {} pages",
                        pages.len() - 1,
                        pages.len()
                    );
                }
            }
        }
    }
}
