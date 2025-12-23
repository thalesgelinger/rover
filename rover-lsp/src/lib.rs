use std::collections::HashMap;
use std::sync::Arc;

use rover_parser::{
    FunctionId, FunctionMetadata, GuardBinding, GuardSchema, GuardType, Route, SemanticModel,
    SourceRange, analyze,
};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Clone, Debug)]
struct DocumentState {
    text: String,
    model: SemanticModel,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn update_document(&self, uri: Url, text: String) {
        let model = analyze(&text);
        {
            let mut docs = self.documents.write().await;
            docs.insert(
                uri.clone(),
                DocumentState {
                    text: text.clone(),
                    model: model.clone(),
                },
            );
        }
        self.publish_diagnostics(&uri, &model).await;
    }

    async fn publish_diagnostics(&self, uri: &Url, model: &SemanticModel) {
        let diagnostics = diagnostics_from_model(model);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".into()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "rover lsp ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut docs = self.documents.write().await;
            docs.remove(&uri);
        }
        self.client
            .publish_diagnostics(uri.clone(), Vec::new(), None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let items = compute_completions(&doc.text, &doc.model, position);
            if items.is_empty() {
                Ok(None)
            } else {
                Ok(Some(CompletionResponse::Array(items)))
            }
        } else {
            Ok(None)
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(hover) = build_hover(&doc.model, position) {
                return Ok(Some(hover));
            }
        }
        Ok(None)
    }
}

fn diagnostics_from_model(model: &SemanticModel) -> Vec<Diagnostic> {
    model
        .errors
        .iter()
        .map(|error| Diagnostic {
            range: source_range_to_range(error.range.as_ref()),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("rover".into()),
            message: error.message.clone(),
            ..Diagnostic::default()
        })
        .collect()
}

fn compute_completions(
    text: &str,
    model: &SemanticModel,
    position: Position,
) -> Vec<CompletionItem> {
    if let Some(function_meta) = find_function(model, &position) {
        if let Some(route) = find_route(model, function_meta.id) {
            if let Some(line_prefix) = line_prefix(text, position) {
                let mut items = Vec::new();
                if let Some((ctx_name, partial)) = detect_params_context(&line_prefix) {
                    if let Some(expected_ctx) = &function_meta.context_param {
                        if &ctx_name == expected_ctx {
                            items.extend(path_param_completions(route, &partial));
                        }
                    }
                }

                if let Some((base, path_segments, partial)) = detect_guard_chain(&line_prefix) {
                    if let Some(binding) = route.guard_bindings.iter().find(|b| b.name == base) {
                        if let Some(properties) = guard_binding_properties(binding, &path_segments)
                        {
                            items.extend(guard_property_completions(properties, &partial));
                        }
                    }
                }

                return items;
            }
        }
    }
    Vec::new()
}

fn build_hover(model: &SemanticModel, position: Position) -> Option<Hover> {
    let function = find_function(model, &position)?;
    let route = find_route(model, function.id)?;
    let mut lines = Vec::new();
    lines.push(format!("**{} {}**", route.method, route.path));
    if let Some(ctx) = &function.context_param {
        lines.push(format!("Context param: `{}`", ctx));
    }
    if !route.request.path_params.is_empty() {
        let params = route
            .request
            .path_params
            .iter()
            .map(|p| format!("`{}`", p.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Path params: {}", params));
    }
    if !route.request.query_params.is_empty() {
        let params = route
            .request
            .query_params
            .iter()
            .map(|p| format!("`{}`", p.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Query params: {}", params));
    }
    if let Some(body) = &route.request.body_schema {
        lines.push(format!("Body schema fields: {}", body.guard_defs.len()));
    }
    if route.responses.is_empty() {
        lines.push("Responses: _none defined_".into());
    } else {
        lines.push("**Responses**".into());
        for response in &route.responses {
            lines.push(format!(
                "- `{}` `{}`",
                response.status, response.content_type
            ));
        }
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: lines.join("\n"),
        }),
        range: Some(source_range_to_range(Some(&function.range))),
    })
}

fn find_function<'a>(
    model: &'a SemanticModel,
    position: &Position,
) -> Option<&'a FunctionMetadata> {
    let line = position.line as usize;
    let column = position.character as usize;
    model
        .functions
        .iter()
        .find(|func| func.range.contains(line, column))
}

fn find_route<'a>(model: &'a SemanticModel, id: FunctionId) -> Option<&'a Route> {
    model
        .server
        .as_ref()
        .and_then(|server| server.routes.iter().find(|route| route.handler == id))
}

fn line_prefix(text: &str, position: Position) -> Option<String> {
    let target_line = position.line as usize;
    for (idx, raw_line) in text.split('\n').enumerate() {
        if idx == target_line {
            let clean_line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let chars: Vec<char> = clean_line.chars().collect();
            let end = usize::min(position.character as usize, chars.len());
            return Some(chars[..end].iter().collect());
        }
    }
    None
}

fn detect_params_context(line: &str) -> Option<(String, String)> {
    const NEEDLE: &str = ":params().";
    if let Some(idx) = line.rfind(NEEDLE) {
        let ctx_ident = extract_identifier_suffix(&line[..idx])?;
        let partial = line[idx + NEEDLE.len()..].to_string();
        return Some((ctx_ident, partial));
    }
    None
}

fn detect_guard_chain(line: &str) -> Option<(String, Vec<String>, String)> {
    let bytes = line.as_bytes();
    let mut start = line.len();
    while start > 0 {
        let b = bytes[start - 1];
        if is_ident_byte(b) || b == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == line.len() {
        return None;
    }
    let chain = &line[start..];
    if !chain.contains('.') {
        return None;
    }
    let mut segments: Vec<&str> = chain.split('.').collect();
    if segments.is_empty() {
        return None;
    }
    let partial = segments.pop().unwrap_or("");
    if segments.is_empty() || segments[0].is_empty() {
        return None;
    }
    let base = segments[0].to_string();
    let path = if segments.len() > 1 {
        segments[1..].iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    };
    Some((base, path, partial.to_string()))
}

fn guard_binding_properties<'a>(
    binding: &'a GuardBinding,
    path: &[String],
) -> Option<&'a HashMap<String, GuardSchema>> {
    if path.is_empty() {
        return Some(&binding.schema);
    }
    let mut current = &binding.schema;
    for (idx, segment) in path.iter().enumerate() {
        let schema = current.get(segment)?;
        match &schema.guard_type {
            GuardType::Object(next) => {
                if idx == path.len() - 1 {
                    return Some(next);
                } else {
                    current = next;
                }
            }
            _ => return None,
        }
    }
    None
}

fn path_param_completions(route: &Route, partial: &str) -> Vec<CompletionItem> {
    let mut params: Vec<String> = route
        .request
        .path_params
        .iter()
        .map(|p| p.name.clone())
        .collect();
    params.sort();
    params
        .into_iter()
        .filter(|name| partial.is_empty() || name.starts_with(partial))
        .map(|name| CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::FIELD),
            detail: Some("path param".into()),
            ..CompletionItem::default()
        })
        .collect()
}

fn guard_property_completions(
    properties: &HashMap<String, GuardSchema>,
    partial: &str,
) -> Vec<CompletionItem> {
    let mut entries: Vec<(&String, &GuardSchema)> = properties.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
        .into_iter()
        .filter(|(name, _)| partial.is_empty() || name.starts_with(partial))
        .map(|(name, schema)| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(guard_type_label(&schema.guard_type)),
            ..CompletionItem::default()
        })
        .collect()
}

fn guard_type_label(guard_type: &GuardType) -> String {
    match guard_type {
        GuardType::String => "string".into(),
        GuardType::Integer => "integer".into(),
        GuardType::Number => "number".into(),
        GuardType::Boolean => "boolean".into(),
        GuardType::Array(inner) => format!("array<{}>", guard_type_label(&inner.guard_type)),
        GuardType::Object(_) => "object".into(),
    }
}

fn extract_identifier_suffix(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut end = segment.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let b = bytes[start - 1];
        if is_ident_byte(b) {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        None
    } else {
        Some(segment[start..end].to_string())
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn source_range_to_range(range: Option<&SourceRange>) -> Range {
    if let Some(r) = range {
        Range {
            start: Position {
                line: r.start.line as u32,
                character: r.start.column as u32,
            },
            end: Position {
                line: r.end.line as u32,
                character: r.end.column as u32,
            },
        }
    } else {
        Range::default()
    }
}

pub async fn start_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

pub fn start_server() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("ROVER_LSP_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _ = runtime.block_on(start_lsp());
}
