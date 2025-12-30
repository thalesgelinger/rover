use std::collections::HashMap;
use std::sync::Arc;

use rover_parser::{
    FunctionId, FunctionMetadata, GuardBinding, GuardSchema, GuardType, MemberKind, Route, SemanticModel,
    SourceRange, SpecDoc, SymbolSpecMember, SymbolSpecMetadata, analyze, lookup_spec,
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
        // TODO: Add debouncing (75ms) using tokio::time::sleep and channel
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
                    trigger_characters: Some(vec![".".into(), ":".into()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
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
            if let Some(hover) = build_hover(&doc.model, &doc.text, position) {
                return Ok(Some(hover));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(location) = find_definition(&doc.model, &doc.text, position, uri.clone()) {
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }
        }
        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let locations = find_references(&doc.text, position, uri.clone(), params.context.include_declaration);
            if !locations.is_empty() {
                return Ok(Some(locations));
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
    let line_prefix = match line_prefix(text, position) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut items = Vec::new();
    
    // Table constructor completions (rover.server { ... })
    if let Some((constructor, partial)) = detect_table_constructor_context(&line_prefix) {
        if constructor == "rover.server" {
            if let Some(config_spec) = lookup_spec("rover_server_config") {
                items.extend(spec_doc_completions(&config_spec, &partial));
            }
        }
        if !items.is_empty() {
            return items;
        }
    }

    // Symbol spec completions (rover., ctx:, g., etc.)
    if let Some((base, partial)) = detect_member_access(&line_prefix) {
        // Collect from local symbol specs
        if let Some(spec) = model.symbol_specs.get(&base) {
            items.extend(symbol_spec_completions(spec, &partial));
        }
        
        // Also try global spec registry for known identifiers
        if items.is_empty() {
            if let Some(spec_doc) = lookup_spec(&base) {
                items.extend(spec_doc_completions(&spec_doc, &partial));
            }
        }
        
        // Add user-defined members (e.g., api.users.get)
        if let Some(members) = model.dynamic_members.get(&base) {
            items.extend(user_defined_member_completions(members, &partial));
        }
        
        return items;
    }
    
    // Global identifier completions (rover, etc.)
    // When user types "rov" suggest "rover"
    if !line_prefix.is_empty() && !line_prefix.contains('.') && !line_prefix.contains(':') {
        let partial = extract_partial_identifier(&line_prefix);
        if !partial.is_empty() {
            items.extend(global_identifier_completions(&model, &partial));
        }
        if !items.is_empty() {
            return items;
        }
    }

    // Chained member access (e.g., rover.guard: -> guard methods)
    if let Some((base, member, partial)) = detect_chained_access(&line_prefix) {
        // Look up base in model's symbol specs
        if let Some(spec) = model.symbol_specs.get(&base) {
            // Find the member and get its target spec
            if let Some(member_spec) = spec.members.iter().find(|m| m.name == member) {
                if let Some(target_doc) = lookup_spec(&member_spec.target_spec_id) {
                    items.extend(spec_doc_completions(&target_doc, &partial));
                }
            }
        }
        // Also check global registry
        if items.is_empty() {
            if let Some(base_doc) = lookup_spec(&base) {
                if let Some(member_doc) = base_doc.members.iter().find(|m| m.name == member) {
                    if let Some(target_doc) = lookup_spec(member_doc.target) {
                        items.extend(spec_doc_completions(&target_doc, &partial));
                    }
                }
            }
        }
    }

    // Handler-specific completions
    if let Some(function_meta) = find_function(model, &position) {
        if let Some(route) = find_route(model, function_meta.id) {
            // ctx:params().xxx completions
            if let Some((ctx_name, partial)) = detect_params_context(&line_prefix) {
                if let Some(expected_ctx) = &function_meta.context_param {
                    if &ctx_name == expected_ctx {
                        items.extend(path_param_completions(route, &partial));
                    }
                }
            }

            // guard binding chain completions (body.xxx)
            if let Some((base, path_segments, partial)) = detect_guard_chain(&line_prefix) {
                if let Some(binding) = route.guard_bindings.iter().find(|b| b.name == base) {
                    if let Some(properties) = guard_binding_properties(binding, &path_segments) {
                        items.extend(guard_property_completions(properties, &partial));
                    }
                }
            }
        }
    }

    items
}

fn build_hover(model: &SemanticModel, text: &str, position: Position) -> Option<Hover> {
    if let Some(hover) = build_symbol_hover(model, text, position) {
        return Some(hover);
    }
    build_route_hover(model, position)
}

fn build_symbol_hover(
    model: &SemanticModel,
    text: &str,
    position: Position,
) -> Option<Hover> {
    let (identifier, range) = identifier_at_position(text, position)?;
    
    // First try local symbol specs from the model
    if let Some(spec) = model.symbol_specs.get(&identifier) {
        let mut lines = Vec::new();
        lines.push(format!("**{}**", identifier));
        if !spec.doc.is_empty() {
            lines.push(spec.doc.clone());
        }
        if !spec.members.is_empty() {
            lines.push("**Members**".into());
            for member in &spec.members {
                let detail = if member.doc.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", member.doc)
                };
                lines.push(format!("- `{}`{}", member.name, detail));
            }
        }

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: lines.join("\n"),
            }),
            range: Some(range),
        });
    }
    
    // Fallback to global spec registry
    if let Some(spec_doc) = lookup_spec(&identifier) {
        let mut lines = Vec::new();
        lines.push(format!("**{}**", identifier));
        if !spec_doc.doc.is_empty() {
            lines.push(spec_doc.doc.to_string());
        }
        if !spec_doc.members.is_empty() {
            lines.push("**Members**".into());
            for member in &spec_doc.members {
                let detail = if member.doc.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", member.doc)
                };
                lines.push(format!("- `{}`{}", member.name, detail));
            }
        }

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: lines.join("\n"),
            }),
            range: Some(range),
        });
    }

    None
}

fn build_route_hover(model: &SemanticModel, position: Position) -> Option<Hover> {
    let function = find_function(model, &position)?;
    if position.line as usize != function.range.start.line {
        return None;
    }
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

fn find_definition(
    model: &SemanticModel,
    text: &str,
    position: Position,
    uri: Url,
) -> Option<Location> {
    // Extract the identifier at the cursor position
    let (identifier, _) = identifier_at_position(text, position)?;
    
    // Try to resolve the symbol in the symbol table
    let line = position.line as usize;
    let column = position.character as usize;
    if let Some(symbol) = model.symbol_table.resolve_symbol_at_position(&identifier, line, column) {
        return Some(Location {
            uri,
            range: Range {
                start: Position {
                    line: symbol.range.start.line as u32,
                    character: symbol.range.start.column as u32,
                },
                end: Position {
                    line: symbol.range.end.line as u32,
                    character: symbol.range.end.column as u32,
                },
            },
        });
    }
    
    None
}

fn find_references(
    text: &str,
    position: Position,
    uri: Url,
    include_declaration: bool,
) -> Vec<Location> {
    // Extract the identifier at the cursor position
    let (identifier, _) = match identifier_at_position(text, position) {
        Some(result) => result,
        None => return Vec::new(),
    };
    
    let mut locations = Vec::new();
    
    // Search through all lines for occurrences of the identifier
    for (line_idx, line) in text.split('\n').enumerate() {
        let clean_line = line.strip_suffix('\r').unwrap_or(line);
        let bytes = clean_line.as_bytes();
        
        let mut col = 0;
        while col < clean_line.len() {
            // Find potential identifier start
            if col == 0 || !is_ident_byte(bytes[col - 1]) {
                let remaining = &clean_line[col..];
                if remaining.starts_with(&identifier) {
                    // Check that it's a complete identifier (not part of a longer word)
                    let end_col = col + identifier.len();
                    let is_complete = end_col >= clean_line.len() 
                        || !is_ident_byte(bytes[end_col]);
                    
                    if is_complete {
                        locations.push(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position {
                                    line: line_idx as u32,
                                    character: col as u32,
                                },
                                end: Position {
                                    line: line_idx as u32,
                                    character: end_col as u32,
                                },
                            },
                        });
                    }
                    col = end_col;
                    continue;
                }
            }
            col += 1;
        }
    }
    
    // Filter out declaration if requested
    if !include_declaration && !locations.is_empty() {
        // The first occurrence is often the declaration, but this is a simplification
        // In practice, we'd need to check against the symbol table
        // For now, return all locations
    }
    
    locations
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

fn detect_table_constructor_context(line: &str) -> Option<(String, String)> {
    // Check if we're inside a table constructor: "rover.server {" or "rover.server { port"
    // Pattern: <identifier>.<identifier> { <partial>
    if !line.contains('{') {
        return None;
    }
    
    let brace_idx = line.rfind('{')?;
    let before_brace = line[..brace_idx].trim();
    let after_brace = line[brace_idx + 1..].trim();
    
    // Extract the constructor call (e.g., "rover.server")
    let parts: Vec<&str> = before_brace.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    
    let constructor = parts.last()?.to_string();
    
    // Extract partial identifier after the brace
    // Handle cases like "{ p" or "{ port = 42, h"
    let partial = if let Some(comma_idx) = after_brace.rfind(',') {
        // After comma, extract last partial identifier
        let after_comma = after_brace[comma_idx + 1..].trim();
        extract_field_partial(after_comma)
    } else {
        // First field in table
        extract_field_partial(after_brace)
    };
    
    Some((constructor, partial))
}

fn extract_field_partial(segment: &str) -> String {
    // Extract partial field name, stop at '=' if present
    let up_to_equals = if let Some(eq_idx) = segment.find('=') {
        &segment[..eq_idx]
    } else {
        segment
    };
    
    up_to_equals.trim().to_string()
}

fn detect_member_access(line: &str) -> Option<(String, String)> {
    if line.is_empty() {
        return None;
    }
    let bytes = line.as_bytes();

    // Walk back over partial identifier (may be empty if cursor right after '.' or ':')
    let mut idx = line.len();
    while idx > 0 && is_ident_byte(bytes[idx - 1]) {
        idx -= 1;
    }

    // Need at least one char before the partial for the separator
    if idx == 0 {
        return None;
    }

    let separator_index = idx - 1;
    let separator = bytes[separator_index];
    if separator != b'.' && separator != b':' {
        return None;
    }

    // Walk back to find the base identifier
    let mut base_start = separator_index;
    while base_start > 0 && is_ident_byte(bytes[base_start - 1]) {
        base_start -= 1;
    }
    if base_start == separator_index {
        return None;
    }

    let base = line[base_start..separator_index].to_string();
    let partial = line[idx..].to_string();
    Some((base, partial))
}

/// Detect chained access like `rover.guard:` -> ("rover", "guard", "")
/// or `rover.guard:str` -> ("rover", "guard", "str")
fn detect_chained_access(line: &str) -> Option<(String, String, String)> {
    if line.is_empty() {
        return None;
    }
    let bytes = line.as_bytes();

    // Walk back over partial identifier
    let mut idx = line.len();
    while idx > 0 && is_ident_byte(bytes[idx - 1]) {
        idx -= 1;
    }

    // Need separator (. or :)
    if idx == 0 {
        return None;
    }
    let sep2_index = idx - 1;
    let sep2 = bytes[sep2_index];
    if sep2 != b'.' && sep2 != b':' {
        return None;
    }

    // Walk back over member identifier
    let mut member_start = sep2_index;
    while member_start > 0 && is_ident_byte(bytes[member_start - 1]) {
        member_start -= 1;
    }
    if member_start == sep2_index {
        return None;
    }

    // Need another separator before member
    if member_start == 0 {
        return None;
    }
    let sep1_index = member_start - 1;
    let sep1 = bytes[sep1_index];
    if sep1 != b'.' && sep1 != b':' {
        return None;
    }

    // Walk back over base identifier
    let mut base_start = sep1_index;
    while base_start > 0 && is_ident_byte(bytes[base_start - 1]) {
        base_start -= 1;
    }
    if base_start == sep1_index {
        return None;
    }

    let base = line[base_start..sep1_index].to_string();
    let member = line[member_start..sep2_index].to_string();
    let partial = line[idx..].to_string();
    Some((base, member, partial))
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

fn symbol_spec_completions(
    spec: &SymbolSpecMetadata,
    partial: &str,
) -> Vec<CompletionItem> {
    let mut members: Vec<&SymbolSpecMember> = spec.members.iter().collect();
    members.sort_by(|a, b| a.name.cmp(&b.name));
    members
        .into_iter()
        .filter(|member| partial.is_empty() || member.name.starts_with(partial))
        .map(|member| CompletionItem {
            label: member.name.clone(),
            kind: Some(match member.kind {
                MemberKind::Field => CompletionItemKind::FIELD,
                MemberKind::Method => CompletionItemKind::METHOD,
            }),
            detail: if member.doc.is_empty() {
                None
            } else {
                Some(member.doc.clone())
            },
            ..CompletionItem::default()
        })
        .collect()
}

fn spec_doc_completions(spec: &SpecDoc, partial: &str) -> Vec<CompletionItem> {
    let mut members: Vec<_> = spec.members.iter().collect();
    members.sort_by(|a, b| a.name.cmp(&b.name));
    members
        .into_iter()
        .filter(|member| partial.is_empty() || member.name.starts_with(partial))
        .map(|member| CompletionItem {
            label: member.name.to_string(),
            kind: Some(match member.kind {
                MemberKind::Field => CompletionItemKind::FIELD,
                MemberKind::Method => CompletionItemKind::METHOD,
            }),
            detail: if member.doc.is_empty() {
                None
            } else {
                Some(member.doc.to_string())
            },
            ..CompletionItem::default()
        })
        .collect()
}

fn global_identifier_completions(model: &SemanticModel, partial: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    
    // Add all known symbols from the model
    for (name, spec) in &model.symbol_specs {
        if partial.is_empty() || name.starts_with(partial) {
            items.push(CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: if spec.doc.is_empty() {
                    None
                } else {
                    Some(spec.doc.clone())
                },
                ..CompletionItem::default()
            });
        }
    }
    
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

fn user_defined_member_completions(members: &[String], partial: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    
    for member in members {
        if partial.is_empty() || member.starts_with(partial) {
            items.push(CompletionItem {
                label: member.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("user-defined function".into()),
                ..CompletionItem::default()
            });
        }
    }
    
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

fn extract_partial_identifier(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut end = line.len();
    
    // Walk back to find start of identifier
    while end > 0 && !bytes[end - 1].is_ascii_whitespace() && bytes[end - 1] != b'(' && bytes[end - 1] != b'{' {
        end -= 1;
    }
    
    line[end..].trim().to_string()
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

fn identifier_at_position(text: &str, position: Position) -> Option<(String, Range)> {
    let line_index = position.line as usize;
    let line = text.split('\n').nth(line_index)?;
    let clean_line = line.strip_suffix('\r').unwrap_or(line);
    if clean_line.is_empty() {
        return None;
    }
    let bytes = clean_line.as_bytes();
    let mut idx = usize::min(position.character as usize, clean_line.len());
    if idx == clean_line.len() {
        if idx == 0 {
            return None;
        }
        idx -= 1;
    }
    if !is_ident_byte(bytes.get(idx).copied().unwrap_or(b' ')) {
        while idx > 0 && !is_ident_byte(bytes[idx]) {
            idx -= 1;
        }
        if !is_ident_byte(bytes[idx]) {
            return None;
        }
    }
    let mut start = idx;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = idx + 1;
    while end < clean_line.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let ident = clean_line[start..end].to_string();
    Some((
        ident,
        Range {
            start: Position {
                line: position.line,
                character: start as u32,
            },
            end: Position {
                line: position.line,
                character: end as u32,
            },
        },
    ))
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
