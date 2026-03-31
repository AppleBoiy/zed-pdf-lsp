// Document Registry
// This module maintains a registry of currently open PDF documents

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tower_lsp::lsp_types::Url;
use anyhow::Result;

pub struct DocumentRegistry {
    pub(crate) documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

#[allow(dead_code)]
pub struct DocumentState {
    pub uri: Url,
    pub opened_at: Instant,
    pub content_hash: Option<u64>,
}

impl DocumentRegistry {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, uri: Url) -> Result<()> {
        let mut documents = self.documents.write().unwrap();
        let state = DocumentState {
            uri: uri.clone(),
            opened_at: Instant::now(),
            content_hash: None,
        };
        documents.insert(uri, state);
        Ok(())
    }

    pub fn unregister(&self, uri: &Url) -> Result<()> {
        let mut documents = self.documents.write().unwrap();
        documents.remove(uri);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_open(&self, uri: &Url) -> bool {
        let documents = self.documents.read().unwrap();
        documents.contains_key(uri)
    }

    #[allow(dead_code)]
    pub fn get_all_open(&self) -> Vec<Url> {
        let documents = self.documents.read().unwrap();
        documents.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_new_registry_is_empty() {
        let registry = DocumentRegistry::new();
        assert_eq!(registry.get_all_open().len(), 0);
    }

    #[test]
    fn test_register_document() {
        let registry = DocumentRegistry::new();
        let uri = Url::from_str("file:///test.pdf").unwrap();
        
        let result = registry.register(uri.clone());
        assert!(result.is_ok());
        assert!(registry.is_open(&uri));
    }

    #[test]
    fn test_unregister_document() {
        let registry = DocumentRegistry::new();
        let uri = Url::from_str("file:///test.pdf").unwrap();
        
        registry.register(uri.clone()).unwrap();
        assert!(registry.is_open(&uri));
        
        let result = registry.unregister(&uri);
        assert!(result.is_ok());
        assert!(!registry.is_open(&uri));
    }

    #[test]
    fn test_is_open_returns_false_for_unregistered() {
        let registry = DocumentRegistry::new();
        let uri = Url::from_str("file:///test.pdf").unwrap();
        
        assert!(!registry.is_open(&uri));
    }

    #[test]
    fn test_get_all_open_returns_all_registered() {
        let registry = DocumentRegistry::new();
        let uri1 = Url::from_str("file:///test1.pdf").unwrap();
        let uri2 = Url::from_str("file:///test2.pdf").unwrap();
        let uri3 = Url::from_str("file:///test3.pdf").unwrap();
        
        registry.register(uri1.clone()).unwrap();
        registry.register(uri2.clone()).unwrap();
        registry.register(uri3.clone()).unwrap();
        
        let all_open = registry.get_all_open();
        assert_eq!(all_open.len(), 3);
        assert!(all_open.contains(&uri1));
        assert!(all_open.contains(&uri2));
        assert!(all_open.contains(&uri3));
    }

    #[test]
    fn test_multiple_documents_simultaneously() {
        let registry = DocumentRegistry::new();
        let uri1 = Url::from_str("file:///doc1.pdf").unwrap();
        let uri2 = Url::from_str("file:///doc2.pdf").unwrap();
        
        registry.register(uri1.clone()).unwrap();
        registry.register(uri2.clone()).unwrap();
        
        assert!(registry.is_open(&uri1));
        assert!(registry.is_open(&uri2));
        assert_eq!(registry.get_all_open().len(), 2);
        
        registry.unregister(&uri1).unwrap();
        assert!(!registry.is_open(&uri1));
        assert!(registry.is_open(&uri2));
        assert_eq!(registry.get_all_open().len(), 1);
    }

    #[test]
    fn test_document_state_fields() {
        let registry = DocumentRegistry::new();
        let uri = Url::from_str("file:///test.pdf").unwrap();
        
        let before = Instant::now();
        registry.register(uri.clone()).unwrap();
        let after = Instant::now();
        
        // Verify the document is registered
        assert!(registry.is_open(&uri));
        
        // Access the document state to verify fields
        let documents = registry.documents.read().unwrap();
        let state = documents.get(&uri).unwrap();
        
        assert_eq!(state.uri, uri);
        assert!(state.opened_at >= before && state.opened_at <= after);
        assert_eq!(state.content_hash, None);
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;
        
        let registry = DocumentRegistry::new();
        let registry_clone = DocumentRegistry {
            documents: Arc::clone(&registry.documents),
        };
        
        let uri1 = Url::from_str("file:///thread1.pdf").unwrap();
        let uri2 = Url::from_str("file:///thread2.pdf").unwrap();
        
        let handle1 = thread::spawn(move || {
            registry.register(uri1).unwrap();
        });
        
        let handle2 = thread::spawn(move || {
            registry_clone.register(uri2).unwrap();
        });
        
        handle1.join().unwrap();
        handle2.join().unwrap();
    }

    // Task 10.7: Performance benchmarks for Document Registry
    // **Validates: Requirements 7.1, 7.2, 7.3, 7.4**
    mod benchmarks {
        use super::*;
        use std::str::FromStr;
        use std::time::Instant;

        #[test]
        fn bench_register_latency() {
            let registry = DocumentRegistry::new();
            let uri = Url::from_str("file:///bench/test.pdf").unwrap();

            let start = Instant::now();
            for _ in 0..1000 {
                registry.register(uri.clone()).unwrap();
            }
            let elapsed = start.elapsed();
            let avg_ms = elapsed.as_millis() as f64 / 1000.0;

            // Register should be < 100ms per operation (Req 8.3)
            assert!(
                avg_ms < 100.0,
                "Register avg latency {:.3}ms, expected < 100ms",
                avg_ms
            );
        }

        #[test]
        fn bench_unregister_latency() {
            let registry = DocumentRegistry::new();
            let uri = Url::from_str("file:///bench/test.pdf").unwrap();
            registry.register(uri.clone()).unwrap();

            let start = Instant::now();
            for _ in 0..1000 {
                registry.unregister(&uri).unwrap();
                registry.register(uri.clone()).unwrap();
            }
            let elapsed = start.elapsed();
            // Each iteration does one unregister + one register
            let avg_ms = elapsed.as_millis() as f64 / 1000.0;

            // Unregister + register cycle should be < 100ms
            assert!(
                avg_ms < 100.0,
                "Unregister+register cycle avg latency {:.3}ms, expected < 100ms",
                avg_ms
            );
        }

        #[test]
        fn bench_is_open_lookup_performance() {
            let registry = DocumentRegistry::new();

            // Pre-populate with 100 documents
            for i in 0..100 {
                let uri = Url::from_str(&format!("file:///bench/doc{}.pdf", i)).unwrap();
                registry.register(uri).unwrap();
            }

            let lookup_uri = Url::from_str("file:///bench/doc50.pdf").unwrap();
            let missing_uri = Url::from_str("file:///bench/missing.pdf").unwrap();

            let start = Instant::now();
            for _ in 0..10_000 {
                let _ = registry.is_open(&lookup_uri);
                let _ = registry.is_open(&missing_uri);
            }
            let elapsed = start.elapsed();
            let avg_us = elapsed.as_micros() as f64 / 20_000.0; // 2 lookups per iter

            // Lookup should be sub-millisecond
            assert!(
                avg_us < 1000.0,
                "is_open avg latency {:.1}µs, expected < 1000µs",
                avg_us
            );
        }

        #[test]
        fn bench_registry_with_many_documents() {
            let registry = DocumentRegistry::new();

            // Register 100+ documents and verify operations remain fast
            let start = Instant::now();
            for i in 0..200 {
                let uri = Url::from_str(&format!("file:///bench/doc{}.pdf", i)).unwrap();
                registry.register(uri).unwrap();
            }
            let register_elapsed = start.elapsed();

            assert_eq!(registry.get_all_open().len(), 200);

            // All 200 registrations should complete well under 1 second
            assert!(
                register_elapsed.as_secs_f64() < 1.0,
                "Registering 200 documents took {:.3}s, expected < 1s",
                register_elapsed.as_secs_f64()
            );

            // Lookup in a 200-document registry
            let uri = Url::from_str("file:///bench/doc150.pdf").unwrap();
            let start = Instant::now();
            for _ in 0..10_000 {
                let _ = registry.is_open(&uri);
            }
            let lookup_elapsed = start.elapsed();
            let avg_us = lookup_elapsed.as_micros() as f64 / 10_000.0;

            assert!(
                avg_us < 1000.0,
                "is_open with 200 docs avg {:.1}µs, expected < 1000µs",
                avg_us
            );

            // Unregister all 200 documents
            let start = Instant::now();
            for i in 0..200 {
                let uri = Url::from_str(&format!("file:///bench/doc{}.pdf", i)).unwrap();
                registry.unregister(&uri).unwrap();
            }
            let unregister_elapsed = start.elapsed();

            assert_eq!(registry.get_all_open().len(), 0);
            assert!(
                unregister_elapsed.as_secs_f64() < 1.0,
                "Unregistering 200 documents took {:.3}s, expected < 1s",
                unregister_elapsed.as_secs_f64()
            );
        }

        #[test]
        fn bench_concurrent_registry_operations() {
            use std::thread;

            let registry = DocumentRegistry::new();
            let shared_docs = Arc::clone(&registry.documents);

            let num_threads = 4;
            let ops_per_thread = 100;

            let start = Instant::now();

            let handles: Vec<_> = (0..num_threads)
                .map(|t| {
                    let docs = Arc::clone(&shared_docs);
                    thread::spawn(move || {
                        let reg = DocumentRegistry { documents: docs };
                        for i in 0..ops_per_thread {
                            let uri = Url::from_str(&format!(
                                "file:///concurrent/t{}_doc{}.pdf",
                                t, i
                            ))
                            .unwrap();
                            reg.register(uri.clone()).unwrap();
                            let _ = reg.is_open(&uri);
                            reg.unregister(&uri).unwrap();
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().expect("Thread panicked");
            }

            let elapsed = start.elapsed();

            // 4 threads × 100 register/lookup/unregister cycles should be fast
            assert!(
                elapsed.as_secs_f64() < 5.0,
                "Concurrent registry ops ({} threads × {} ops) took {:.2}s, expected < 5s",
                num_threads,
                ops_per_thread,
                elapsed.as_secs_f64()
            );

            // Registry should be empty after all threads complete
            assert_eq!(registry.get_all_open().len(), 0);
        }
    }
}
