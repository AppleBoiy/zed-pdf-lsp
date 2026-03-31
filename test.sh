#!/bin/bash
BINARY="./target/release/zed-pdf-lsp"

INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{},"rootUri":"file:///"}}'
INITIALIZED='{"jsonrpc":"2.0","method":"initialized","params":{}}'
OPEN='{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///Users/ive/works/zed-pdf-lsp/example.pdf","languageId":"pdf","version":1,"text":""}}}'

{
  printf "Content-Length: %d\r\n\r\n%s" "${#INIT}" "$INIT"
  printf "Content-Length: %d\r\n\r\n%s" "${#INITIALIZED}" "$INITIALIZED"
  printf "Content-Length: %d\r\n\r\n%s" "${#OPEN}" "$OPEN"
  sleep 3
} | $BINARY
