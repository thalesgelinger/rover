use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rover_parser::{
    FunctionId, FunctionMetadata, GuardBinding, GuardSchema, GuardType, MemberKind, Route, SemanticModel,
    SourceRange, SpecDoc, SymbolSpecMember, SymbolSpecMetadata, analyze, analyze_with_options,
    lookup_spec, LuaType,
};

// Alias to avoid collision with rover_parser::SymbolKind
use tower_lsp::lsp_types::SymbolKind as LspSymbolKind;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const DEBOUNCE_MS: u64 = 75;

// Semantic token types - order matters (index used in token data)
const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,    // 0: modules, require
    SemanticTokenType::TYPE,         // 1: types
    SemanticTokenType::CLASS,        // 2: rover server/guard
    SemanticTokenType::FUNCTION,     // 3: functions
    SemanticTokenType::METHOD,       // 4: methods
    SemanticTokenType::PROPERTY,     // 5: table fields
    SemanticTokenType::VARIABLE,     // 6: variables
    SemanticTokenType::PARAMETER,    // 7: parameters
    SemanticTokenType::STRING,       // 8: strings
    SemanticTokenType::NUMBER,       // 9: numbers
    SemanticTokenType::KEYWORD,      // 10: keywords
    SemanticTokenType::COMMENT,      // 11: comments
    SemanticTokenType::OPERATOR,     // 12: operators
];

const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,   // 0: declaration site
    SemanticTokenModifier::DEFINITION,    // 1: definition site
    SemanticTokenModifier::READONLY,      // 2: constants
    SemanticTokenModifier::STATIC,        // 3: static/global
    SemanticTokenModifier::DEFAULT_LIBRARY, // 4: stdlib
];

#[derive(Clone, Debug)]
struct DocumentState {
    text: String,
    model: SemanticModel,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
    /// Version counters for debouncing - tracks latest version per document
    update_versions: Arc<RwLock<HashMap<Url, u64>>>,
    /// Global counter for generating unique versions
    version_counter: Arc<AtomicU64>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            update_versions: Arc::new(RwLock::new(HashMap::new())),
            version_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    async fn update_document(&self, uri: Url, text: String) {
        // Assign a version to this update
        let version = self.version_counter.fetch_add(1, Ordering::SeqCst);
        
        // Store the latest version for this document
        {
            let mut versions = self.update_versions.write().await;
            versions.insert(uri.clone(), version);
        }

        // Clone what we need for the spawned task
        let client = self.client.clone();
        let documents = self.documents.clone();
        let update_versions = self.update_versions.clone();
        let uri_clone = uri.clone();

        // Spawn debounced update task
        tokio::spawn(async move {
            // Wait for debounce period
            tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;

            // Check if this is still the latest version for this document
            let current_version = {
                let versions = update_versions.read().await;
                versions.get(&uri_clone).copied()
            };

            // If a newer update came in, skip this one
            if current_version != Some(version) {
                return;
            }

            // Perform the actual analysis with type inference
            use rover_parser::AnalyzeOptions;
            let model = analyze_with_options(&text, AnalyzeOptions { type_inference: true });
            {
                let mut docs = documents.write().await;
                docs.insert(
                    uri_clone.clone(),
                    DocumentState {
                        text: text.clone(),
                        model: model.clone(),
                    },
                );
            }

            // Publish diagnostics
            let diagnostics = diagnostics_from_model(&model);
            client
                .publish_diagnostics(uri_clone, diagnostics, None)
                .await;
        });
    }

    /// Update document immediately without debouncing (for did_open)
    async fn update_document_immediate(&self, uri: Url, text: String) {
        use rover_parser::AnalyzeOptions;
        let model = analyze_with_options(&text, AnalyzeOptions { type_inference: true });
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
                document_symbol_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_formatting_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                            token_modifiers: SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        work_done_progress_options: Default::default(),
                    }),
                ),
                document_highlight_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
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
        // Immediate update on open - user expects instant feedback
        self.update_document_immediate(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            // Debounced update on change - reduces CPU during typing
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
        {
            let mut versions = self.update_versions.write().await;
            versions.remove(&uri);
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

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let symbols = build_document_symbols(&doc.model);
            if !symbols.is_empty() {
                return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
            }
        }
        Ok(None)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some((identifier, range)) = identifier_at_position(&doc.text, position) {
                // Check if it's a renameable symbol (not a keyword or stdlib)
                if is_lua_keyword(&identifier) {
                    return Err(tower_lsp::jsonrpc::Error::new(
                        tower_lsp::jsonrpc::ErrorCode::InvalidRequest,
                    ));
                }
                if is_stdlib_global(&identifier) {
                    return Err(tower_lsp::jsonrpc::Error::new(
                        tower_lsp::jsonrpc::ErrorCode::InvalidRequest,
                    ));
                }
                return Ok(Some(PrepareRenameResponse::Range(range)));
            }
        }
        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        // Validate new name
        if new_name.is_empty() || is_lua_keyword(&new_name) {
            return Err(tower_lsp::jsonrpc::Error::new(
                tower_lsp::jsonrpc::ErrorCode::InvalidParams,
            ));
        }

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(edit) = compute_rename(&doc.text, position, &new_name, uri.clone()) {
                return Ok(Some(edit));
            }
        }
        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(help) = compute_signature_help(&doc.text, &doc.model, position) {
                return Ok(Some(help));
            }
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            if let Some(formatted) = format_document(&doc.text) {
                let lines: Vec<&str> = doc.text.split('\n').collect();
                let last_line = lines.len().saturating_sub(1);
                let last_col = lines.last().map(|l| l.len()).unwrap_or(0);
                
                return Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position { line: 0, character: 0 },
                        end: Position {
                            line: last_line as u32,
                            character: last_col as u32,
                        },
                    },
                    new_text: formatted,
                }]));
            }
        }
        Ok(None)
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let ranges = compute_folding_ranges(&doc.text);
            if !ranges.is_empty() {
                return Ok(Some(ranges));
            }
        }
        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let actions = compute_code_actions(&doc.text, &doc.model, range, uri.clone());
            if !actions.is_empty() {
                return Ok(Some(actions));
            }
        }
        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let docs = self.documents.read().await;
        let mut symbols = vec![];

        for (uri, doc) in docs.iter() {
            // Get functions from model
            for func in &doc.model.functions {
                if query.is_empty() || func.name.to_lowercase().contains(&query) {
                    #[allow(deprecated)]
                    symbols.push(SymbolInformation {
                        name: func.name.clone(),
                        kind: LspSymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri: uri.clone(),
                            range: source_range_to_range(Some(&func.range)),
                        },
                        container_name: None,
                    });
                }
            }

            // Get symbols from symbol table
            for symbol in doc.model.symbol_table.all_symbols() {
                if query.is_empty() || symbol.name.to_lowercase().contains(&query) {
                    let kind = match symbol.kind {
                        rover_parser::SymbolKind::Function => LspSymbolKind::FUNCTION,
                        rover_parser::SymbolKind::Variable => LspSymbolKind::VARIABLE,
                        rover_parser::SymbolKind::Parameter => LspSymbolKind::VARIABLE,
                        rover_parser::SymbolKind::Global => LspSymbolKind::VARIABLE,
                        rover_parser::SymbolKind::RoverServer => LspSymbolKind::CLASS,
                        rover_parser::SymbolKind::RoverGuard => LspSymbolKind::CLASS,
                        rover_parser::SymbolKind::ContextParam => LspSymbolKind::VARIABLE,
                        _ => LspSymbolKind::VARIABLE,
                    };

                    #[allow(deprecated)]
                    symbols.push(SymbolInformation {
                        name: symbol.name.clone(),
                        kind,
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri: uri.clone(),
                            range: symbol_range_to_lsp_range(&symbol.range),
                        },
                        container_name: None,
                    });
                }
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let tokens = compute_semantic_tokens(&doc.text, &doc.model);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: tokens,
            })));
        }
        Ok(None)
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let highlights = compute_document_highlights(&doc.text, position);
            if !highlights.is_empty() {
                return Ok(Some(highlights));
            }
        }
        Ok(None)
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let ranges: Vec<SelectionRange> = params
                .positions
                .iter()
                .filter_map(|pos| compute_selection_range(&doc.text, *pos))
                .collect();
            if !ranges.is_empty() {
                return Ok(Some(ranges));
            }
        }
        Ok(None)
    }
}

fn diagnostics_from_model(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = model
        .errors
        .iter()
        .map(|error| Diagnostic {
            range: source_range_to_range(error.range.as_ref()),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("rover".into()),
            message: error.message.clone(),
            ..Diagnostic::default()
        })
        .collect();

    // Add unused variable warnings
    diagnostics.extend(compute_unused_variable_warnings(model));

    diagnostics
}

fn compute_unused_variable_warnings(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut warnings = Vec::new();

    // Get unused symbols from symbol table (already filtered for variables/params, skips _ prefixed)
    for symbol in model.symbol_table.get_unused_symbols() {
        // Skip common parameter names that are often unused (self, cls, ctx in Rover)
        if matches!(symbol.name.as_str(), "self" | "cls") {
            continue;
        }

        let message = match symbol.kind {
            rover_parser::SymbolKind::Parameter => {
                format!("Unused parameter '{}'", symbol.name)
            }
            _ => {
                format!("Unused variable '{}'", symbol.name)
            }
        };

        // Convert symbol range to LSP range
        let range = Range {
            start: Position {
                line: symbol.range.start.line as u32,
                character: symbol.range.start.column as u32,
            },
            end: Position {
                line: symbol.range.end.line as u32,
                character: symbol.range.end.column as u32,
            },
        };

        warnings.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("rover".into()),
            message,
            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
            ..Diagnostic::default()
        });
    }

    warnings
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
    
    // Priority 1: Check symbol table for local variables/parameters
    if let Some(symbol) = model.symbol_table.resolve_symbol_global(&identifier) {
        let mut lines = Vec::new();
        let kind_str = match symbol.kind {
            rover_parser::SymbolKind::Variable => "local variable",
            rover_parser::SymbolKind::Function => "function",
            rover_parser::SymbolKind::Parameter => "parameter",
            rover_parser::SymbolKind::Global => "global",
            rover_parser::SymbolKind::Builtin => "builtin",
            rover_parser::SymbolKind::RoverServer => "rover server",
            rover_parser::SymbolKind::RoverGuard => "rover guard",
            rover_parser::SymbolKind::ContextParam => "context parameter",
        };
        lines.push(format!("**{}** _{}_", identifier, kind_str));
        
        // Show inferred type if not Unknown
        if !matches!(symbol.inferred_type, LuaType::Unknown) {
            lines.push(format!("Inferred type: `{}`", symbol.inferred_type));
        }
        
        if let Some(type_annotation) = &symbol.type_annotation {
            lines.push(format!("Type annotation: `{}`", type_annotation));
        }
        
        lines.push(format!("Defined at line {}", symbol.range.start.line + 1));

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: lines.join("\n\n"),
            }),
            range: Some(range),
        });
    }
    
    // Priority 2: Rover symbol specs from the model
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
    
    // Priority 3: Lua stdlib from global spec registry
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

fn build_document_symbols(model: &SemanticModel) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // Add functions from model
    for func in &model.functions {
        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: func.name.clone(),
            detail: func.context_param.as_ref().map(|ctx| format!("({})", ctx)),
            kind: SymbolKind::FUNCTION,
            range: source_range_to_range(Some(&func.range)),
            selection_range: source_range_to_range(Some(&func.range)),
            children: None,
            tags: None,
            deprecated: None,
        });
    }

    // Add local variables from symbol table
    for symbol in model.symbol_table.all_symbols() {
        let kind = match symbol.kind {
            rover_parser::SymbolKind::Variable => SymbolKind::VARIABLE,
            rover_parser::SymbolKind::Function => SymbolKind::FUNCTION,
            rover_parser::SymbolKind::Parameter => SymbolKind::VARIABLE,
            rover_parser::SymbolKind::Global => SymbolKind::VARIABLE,
            rover_parser::SymbolKind::Builtin => continue, // Skip builtins
            rover_parser::SymbolKind::RoverServer => SymbolKind::OBJECT,
            rover_parser::SymbolKind::RoverGuard => SymbolKind::OBJECT,
            rover_parser::SymbolKind::ContextParam => SymbolKind::VARIABLE,
        };

        let range = Range {
            start: Position {
                line: symbol.range.start.line as u32,
                character: symbol.range.start.column as u32,
            },
            end: Position {
                line: symbol.range.end.line as u32,
                character: symbol.range.end.column as u32,
            },
        };

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: symbol.name.clone(),
            detail: symbol.type_annotation.clone(),
            kind,
            range,
            selection_range: range,
            children: None,
            tags: None,
            deprecated: None,
        });
    }

    // Sort by position
    symbols.sort_by(|a, b| {
        a.range.start.line.cmp(&b.range.start.line)
            .then(a.range.start.character.cmp(&b.range.start.character))
    });

    symbols
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

    // Use AST-based search to find all identifier nodes matching our target
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Failed to load Lua grammar");

    let Some(tree) = parser.parse(text, None) else {
        return vec![];
    };

    let mut locations = Vec::new();
    let mut declaration_range: Option<Range> = None;

    collect_identifier_references(
        tree.root_node(),
        text,
        &identifier,
        &uri,
        &mut locations,
        &mut declaration_range,
    );

    // Filter out declaration if not requested
    if !include_declaration {
        if let Some(decl_range) = declaration_range {
            locations.retain(|loc| loc.range != decl_range);
        }
    }

    locations
}

fn collect_identifier_references(
    node: tree_sitter::Node,
    text: &str,
    target: &str,
    uri: &Url,
    locations: &mut Vec<Location>,
    declaration_range: &mut Option<Range>,
) {
    // Check if this node is an identifier matching our target
    if node.kind() == "identifier" {
        let name = &text[node.start_byte()..node.end_byte()];
        if name == target {
            let start = node.start_position();
            let end = node.end_position();

            let range = Range {
                start: Position {
                    line: start.row as u32,
                    character: start.column as u32,
                },
                end: Position {
                    line: end.row as u32,
                    character: end.column as u32,
                },
            };

            // Check if this is a declaration site
            if is_declaration_site(node) && declaration_range.is_none() {
                *declaration_range = Some(range);
            }

            locations.push(Location {
                uri: uri.clone(),
                range,
            });
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_references(child, text, target, uri, locations, declaration_range);
    }
}

fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    match parent.kind() {
        // Function parameters
        "parameters" => true,

        // Variable declaration (local x = ...)
        "variable_list" => {
            if let Some(gp) = parent.parent() {
                gp.kind() == "variable_declaration"
            } else {
                false
            }
        }

        // For loop variables
        "loop_expression" | "in_clause" => true,

        // Function name in declaration
        "function_declaration" => {
            // Check if this identifier is the function name
            if let Some(name_node) = parent.child_by_field_name("name") {
                name_node.id() == node.id()
            } else {
                false
            }
        }

        _ => false,
    }
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
    
    // Priority 1: Rover constructs from symbol_specs
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
                sort_text: Some(format!("0_{}", name)),
                ..CompletionItem::default()
            });
        }
    }
    
    // Priority 2: Local variables from symbol table
    let mut seen = std::collections::HashSet::new();
    for symbol in model.symbol_table.all_symbols() {
        if (partial.is_empty() || symbol.name.starts_with(partial)) && !seen.contains(&symbol.name) {
            seen.insert(symbol.name.clone());
            
            let kind = match symbol.kind {
                rover_parser::SymbolKind::Function => CompletionItemKind::FUNCTION,
                rover_parser::SymbolKind::Parameter => CompletionItemKind::VARIABLE,
                _ => CompletionItemKind::VARIABLE,
            };
            
            let kind_name = match symbol.kind {
                rover_parser::SymbolKind::Variable => "local variable",
                rover_parser::SymbolKind::Function => "function",
                rover_parser::SymbolKind::Parameter => "parameter",
                rover_parser::SymbolKind::Global => "global",
                rover_parser::SymbolKind::Builtin => "builtin",
                rover_parser::SymbolKind::RoverServer => "rover server",
                rover_parser::SymbolKind::RoverGuard => "rover guard",
                rover_parser::SymbolKind::ContextParam => "context parameter",
            };
            
            // Include inferred type if not Unknown
            let detail = if !matches!(symbol.inferred_type, LuaType::Unknown) {
                format!("{}: {}", kind_name, symbol.inferred_type)
            } else {
                kind_name.to_string()
            };
            
            items.push(CompletionItem {
                label: symbol.name.clone(),
                kind: Some(kind),
                detail: Some(detail),
                sort_text: Some(format!("1_{}", symbol.name)),
                ..CompletionItem::default()
            });
        }
    }
    
    // Priority 3: Lua stdlib globals
    let lua_globals = [
        ("print", "Print values to stdout"),
        ("assert", "Check assertion and raise error if false"),
        ("error", "Raise an error"),
        ("type", "Get type of value"),
        ("tonumber", "Convert to number"),
        ("tostring", "Convert to string"),
        ("ipairs", "Iterator for array-like tables"),
        ("pairs", "Iterator for all table pairs"),
        ("next", "Get next table key-value pair"),
        ("pcall", "Protected call"),
        ("xpcall", "Protected call with error handler"),
        ("require", "Load module"),
        ("string", "String manipulation library"),
        ("table", "Table manipulation library"),
        ("math", "Mathematical functions library"),
        ("io", "I/O library"),
        ("os", "Operating system library"),
        ("coroutine", "Coroutine library"),
        ("debug", "Debug library"),
        ("package", "Package/module system"),
    ];
    
    for (name, doc) in &lua_globals {
        if (partial.is_empty() || name.starts_with(partial)) && !seen.contains(*name) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(doc.to_string()),
                sort_text: Some(format!("2_{}", name)),
                ..CompletionItem::default()
            });
        }
    }
    
    items.sort_by(|a, b| {
        a.sort_text.as_ref().unwrap_or(&a.label)
            .cmp(b.sort_text.as_ref().unwrap_or(&b.label))
    });
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

fn symbol_range_to_lsp_range(range: &rover_parser::SymbolSourceRange) -> Range {
    Range {
        start: Position {
            line: range.start.line as u32,
            character: range.start.column as u32,
        },
        end: Position {
            line: range.end.line as u32,
            character: range.end.column as u32,
        },
    }
}

fn compute_rename(text: &str, position: Position, new_name: &str, uri: Url) -> Option<WorkspaceEdit> {
    let (_identifier, _) = identifier_at_position(text, position)?;
    
    // Find all references to the identifier
    let locations = find_references(text, position, uri.clone(), true);
    if locations.is_empty() {
        return None;
    }
    
    // Create text edits for each reference
    let edits: Vec<TextEdit> = locations
        .into_iter()
        .map(|loc| TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        })
        .collect();
    
    let mut changes = HashMap::new();
    changes.insert(uri, edits);
    
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

fn compute_signature_help(text: &str, model: &SemanticModel, position: Position) -> Option<SignatureHelp> {
    let line_prefix = line_prefix(text, position)?;
    
    // Detect function call context: find the function name before the (
    let (func_name, active_param) = detect_function_call_context(&line_prefix)?;
    
    // Look up function signature
    if let Some(signature) = get_function_signature(&func_name, model) {
        return Some(SignatureHelp {
            signatures: vec![signature],
            active_signature: Some(0),
            active_parameter: Some(active_param),
        });
    }
    
    None
}

fn detect_function_call_context(line: &str) -> Option<(String, u32)> {
    // Find the opening paren that we're inside
    let bytes = line.as_bytes();
    let mut paren_depth: i32 = 0;
    let mut last_open_paren = None;
    let mut comma_count: u32 = 0;
    
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'(' {
            paren_depth += 1;
            last_open_paren = Some(i);
            comma_count = 0;
        } else if b == b')' {
            paren_depth = paren_depth.saturating_sub(1);
            if paren_depth == 0 {
                last_open_paren = None;
            }
        } else if b == b',' && paren_depth > 0 {
            comma_count += 1;
        }
    }
    
    // If we found an open paren, extract the function name before it
    let paren_pos = last_open_paren?;
    if paren_pos == 0 {
        return None;
    }
    
    // Walk back to find the function name (handle both "func(" and "obj:method(" and "obj.method(")
    let end = paren_pos;
    let mut start = end;
    
    while start > 0 {
        let b = bytes[start - 1];
        if is_ident_byte(b) || b == b'.' || b == b':' {
            start -= 1;
        } else {
            break;
        }
    }
    
    if start == end {
        return None;
    }
    
    let func_name = line[start..end].to_string();
    Some((func_name, comma_count))
}

fn get_function_signature(func_name: &str, model: &SemanticModel) -> Option<SignatureInformation> {
    // Check if it's a method call like "string.format" or "table.insert"
    if let Some((base, method)) = func_name.rsplit_once('.') {
        if let Some(sig) = get_stdlib_signature(base, method) {
            return Some(sig);
        }
    }
    
    // Check for colon methods like "ctx:json"
    if let Some((base, method)) = func_name.rsplit_once(':') {
        // Look in model's symbol specs
        if let Some(spec) = model.symbol_specs.get(base) {
            for member in &spec.members {
                if member.name == method {
                    return Some(SignatureInformation {
                        label: format!("{}:{}", base, member.name),
                        documentation: if member.doc.is_empty() {
                            None
                        } else {
                            Some(Documentation::String(member.doc.clone()))
                        },
                        parameters: None,
                        active_parameter: None,
                    });
                }
            }
        }
    }
    
    // Check global functions
    if let Some(sig) = get_global_function_signature(func_name) {
        return Some(sig);
    }
    
    None
}

fn get_global_function_signature(name: &str) -> Option<SignatureInformation> {
    let (label, doc, params) = match name {
        "print" => ("print(...)", "Print values to stdout", vec!["..."]),
        "assert" => ("assert(v, message?)", "Raise error if v is false/nil", vec!["v", "message?"]),
        "error" => ("error(message, level?)", "Raise an error", vec!["message", "level?"]),
        "type" => ("type(v)", "Return type of value as string", vec!["v"]),
        "tonumber" => ("tonumber(v, base?)", "Convert to number", vec!["v", "base?"]),
        "tostring" => ("tostring(v)", "Convert to string", vec!["v"]),
        "ipairs" => ("ipairs(t)", "Iterator for array indices", vec!["t"]),
        "pairs" => ("pairs(t)", "Iterator for all table keys", vec!["t"]),
        "next" => ("next(t, key?)", "Get next key-value pair", vec!["t", "key?"]),
        "pcall" => ("pcall(f, ...)", "Protected call", vec!["f", "..."]),
        "xpcall" => ("xpcall(f, err)", "Protected call with error handler", vec!["f", "err"]),
        "select" => ("select(index, ...)", "Select from varargs", vec!["index", "..."]),
        "getmetatable" => ("getmetatable(obj)", "Get metatable", vec!["obj"]),
        "setmetatable" => ("setmetatable(t, mt)", "Set metatable", vec!["t", "mt"]),
        "rawget" => ("rawget(t, k)", "Get without metamethod", vec!["t", "k"]),
        "rawset" => ("rawset(t, k, v)", "Set without metamethod", vec!["t", "k", "v"]),
        "rawequal" => ("rawequal(a, b)", "Equal without metamethod", vec!["a", "b"]),
        "require" => ("require(modname)", "Load module", vec!["modname"]),
        "load" => ("load(func, chunkname?)", "Load chunk from function", vec!["func", "chunkname?"]),
        "loadfile" => ("loadfile(filename?)", "Load chunk from file", vec!["filename?"]),
        "loadstring" => ("loadstring(s, chunkname?)", "Load chunk from string", vec!["s", "chunkname?"]),
        "dofile" => ("dofile(filename?)", "Execute file", vec!["filename?"]),
        "unpack" => ("unpack(t, i?, j?)", "Unpack table to multiple values", vec!["t", "i?", "j?"]),
        "collectgarbage" => ("collectgarbage(opt?, arg?)", "Control garbage collector", vec!["opt?", "arg?"]),
        _ => return None,
    };
    
    Some(SignatureInformation {
        label: label.to_string(),
        documentation: Some(Documentation::String(doc.to_string())),
        parameters: Some(
            params
                .into_iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(p.to_string()),
                    documentation: None,
                })
                .collect(),
        ),
        active_parameter: None,
    })
}

fn get_stdlib_signature(lib: &str, method: &str) -> Option<SignatureInformation> {
    let (label, doc, params): (&str, &str, Vec<&str>) = match (lib, method) {
        // string library
        ("string", "byte") => ("string.byte(s, i?, j?)", "Get byte values", vec!["s", "i?", "j?"]),
        ("string", "char") => ("string.char(...)", "Build string from bytes", vec!["..."]),
        ("string", "find") => ("string.find(s, pattern, init?, plain?)", "Find pattern", vec!["s", "pattern", "init?", "plain?"]),
        ("string", "format") => ("string.format(fmt, ...)", "Format string", vec!["fmt", "..."]),
        ("string", "gmatch") => ("string.gmatch(s, pattern)", "Global pattern iterator", vec!["s", "pattern"]),
        ("string", "gsub") => ("string.gsub(s, pattern, repl, n?)", "Global substitution", vec!["s", "pattern", "repl", "n?"]),
        ("string", "len") => ("string.len(s)", "String length", vec!["s"]),
        ("string", "lower") => ("string.lower(s)", "To lowercase", vec!["s"]),
        ("string", "upper") => ("string.upper(s)", "To uppercase", vec!["s"]),
        ("string", "match") => ("string.match(s, pattern, init?)", "Pattern match", vec!["s", "pattern", "init?"]),
        ("string", "rep") => ("string.rep(s, n)", "Repeat string", vec!["s", "n"]),
        ("string", "reverse") => ("string.reverse(s)", "Reverse string", vec!["s"]),
        ("string", "sub") => ("string.sub(s, i, j?)", "Substring", vec!["s", "i", "j?"]),
        
        // table library
        ("table", "concat") => ("table.concat(t, sep?, i?, j?)", "Concatenate elements", vec!["t", "sep?", "i?", "j?"]),
        ("table", "insert") => ("table.insert(t, pos?, value)", "Insert element", vec!["t", "pos?", "value"]),
        ("table", "remove") => ("table.remove(t, pos?)", "Remove element", vec!["t", "pos?"]),
        ("table", "sort") => ("table.sort(t, comp?)", "Sort table in-place", vec!["t", "comp?"]),
        ("table", "maxn") => ("table.maxn(t)", "Max numeric index", vec!["t"]),
        
        // math library
        ("math", "abs") => ("math.abs(x)", "Absolute value", vec!["x"]),
        ("math", "acos") => ("math.acos(x)", "Arc cosine", vec!["x"]),
        ("math", "asin") => ("math.asin(x)", "Arc sine", vec!["x"]),
        ("math", "atan") => ("math.atan(x)", "Arc tangent", vec!["x"]),
        ("math", "atan2") => ("math.atan2(y, x)", "Arc tangent of y/x", vec!["y", "x"]),
        ("math", "ceil") => ("math.ceil(x)", "Ceiling", vec!["x"]),
        ("math", "cos") => ("math.cos(x)", "Cosine", vec!["x"]),
        ("math", "deg") => ("math.deg(x)", "Radians to degrees", vec!["x"]),
        ("math", "exp") => ("math.exp(x)", "e^x", vec!["x"]),
        ("math", "floor") => ("math.floor(x)", "Floor", vec!["x"]),
        ("math", "fmod") => ("math.fmod(x, y)", "Float modulo", vec!["x", "y"]),
        ("math", "log") => ("math.log(x)", "Natural log", vec!["x"]),
        ("math", "log10") => ("math.log10(x)", "Log base 10", vec!["x"]),
        ("math", "max") => ("math.max(...)", "Maximum value", vec!["..."]),
        ("math", "min") => ("math.min(...)", "Minimum value", vec!["..."]),
        ("math", "pow") => ("math.pow(x, y)", "x^y", vec!["x", "y"]),
        ("math", "rad") => ("math.rad(x)", "Degrees to radians", vec!["x"]),
        ("math", "random") => ("math.random(m?, n?)", "Random number", vec!["m?", "n?"]),
        ("math", "randomseed") => ("math.randomseed(x)", "Set random seed", vec!["x"]),
        ("math", "sin") => ("math.sin(x)", "Sine", vec!["x"]),
        ("math", "sqrt") => ("math.sqrt(x)", "Square root", vec!["x"]),
        ("math", "tan") => ("math.tan(x)", "Tangent", vec!["x"]),
        
        // io library
        ("io", "open") => ("io.open(filename, mode?)", "Open file", vec!["filename", "mode?"]),
        ("io", "close") => ("io.close(file?)", "Close file", vec!["file?"]),
        ("io", "read") => ("io.read(...)", "Read from stdin", vec!["..."]),
        ("io", "write") => ("io.write(...)", "Write to stdout", vec!["..."]),
        ("io", "lines") => ("io.lines(filename?)", "File line iterator", vec!["filename?"]),
        ("io", "input") => ("io.input(file?)", "Set/get input file", vec!["file?"]),
        ("io", "output") => ("io.output(file?)", "Set/get output file", vec!["file?"]),
        ("io", "flush") => ("io.flush()", "Flush output", vec![]),
        ("io", "type") => ("io.type(obj)", "Check if file handle", vec!["obj"]),
        
        // os library
        ("os", "clock") => ("os.clock()", "CPU time used", vec![]),
        ("os", "date") => ("os.date(format?, time?)", "Format date/time", vec!["format?", "time?"]),
        ("os", "difftime") => ("os.difftime(t2, t1)", "Time difference", vec!["t2", "t1"]),
        ("os", "execute") => ("os.execute(cmd?)", "Execute shell command", vec!["cmd?"]),
        ("os", "exit") => ("os.exit(code?)", "Exit program", vec!["code?"]),
        ("os", "getenv") => ("os.getenv(varname)", "Get environment variable", vec!["varname"]),
        ("os", "remove") => ("os.remove(filename)", "Delete file", vec!["filename"]),
        ("os", "rename") => ("os.rename(old, new)", "Rename file", vec!["old", "new"]),
        ("os", "time") => ("os.time(table?)", "Get time", vec!["table?"]),
        ("os", "tmpname") => ("os.tmpname()", "Temp filename", vec![]),
        
        // coroutine library
        ("coroutine", "create") => ("coroutine.create(f)", "Create coroutine", vec!["f"]),
        ("coroutine", "resume") => ("coroutine.resume(co, ...)", "Resume coroutine", vec!["co", "..."]),
        ("coroutine", "yield") => ("coroutine.yield(...)", "Yield from coroutine", vec!["..."]),
        ("coroutine", "status") => ("coroutine.status(co)", "Get status", vec!["co"]),
        ("coroutine", "wrap") => ("coroutine.wrap(f)", "Wrap as function", vec!["f"]),
        ("coroutine", "running") => ("coroutine.running()", "Get running coroutine", vec![]),
        
        _ => return None,
    };
    
    Some(SignatureInformation {
        label: label.to_string(),
        documentation: Some(Documentation::String(doc.to_string())),
        parameters: Some(
            params
                .into_iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(p.to_string()),
                    documentation: None,
                })
                .collect(),
        ),
        active_parameter: None,
    })
}

fn compute_code_actions(
    text: &str,
    model: &SemanticModel,
    range: Range,
    uri: Url,
) -> CodeActionResponse {
    let mut actions = vec![];

    // Check for errors that intersect with the selection range
    for error in &model.errors {
        let error_range = source_range_to_range(error.range.as_ref());

        // Check if error range intersects with selection
        if ranges_intersect(&error_range, &range) {
            // Quick fix: Add missing 'local' declaration
            if error.message.contains("undeclared") || error.message.contains("undefined") {
                // Extract the variable name from the error message if possible
                if let Some(name) = extract_identifier_from_error(&error.message) {
                    let insert_pos = Position {
                        line: error_range.start.line,
                        character: 0,
                    };

                    // Find proper indentation
                    let indent = get_line_indent(text, error_range.start.line as usize);

                    let edit = TextEdit {
                        range: Range {
                            start: insert_pos,
                            end: insert_pos,
                        },
                        new_text: format!("{}local {}\n", indent, name),
                    };

                    let mut changes = HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Add 'local {}' declaration", name),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![Diagnostic {
                            range: error_range,
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("rover".into()),
                            message: error.message.clone(),
                            ..Default::default()
                        }]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
        }
    }

    // Extract function refactoring (if selection spans multiple statements)
    if range.start.line != range.end.line {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Extract to function".to_string(),
            kind: Some(CodeActionKind::REFACTOR_EXTRACT),
            disabled: Some(CodeActionDisabled {
                reason: "Not yet implemented".to_string(),
            }),
            ..Default::default()
        }));
    }

    actions
}

fn ranges_intersect(a: &Range, b: &Range) -> bool {
    !(a.end.line < b.start.line
        || (a.end.line == b.start.line && a.end.character < b.start.character)
        || b.end.line < a.start.line
        || (b.end.line == a.start.line && b.end.character < a.start.character))
}

fn extract_identifier_from_error(message: &str) -> Option<String> {
    // Try to extract identifier from messages like:
    // "undeclared variable 'foo'"
    // "undefined global 'bar'"
    if let Some(start) = message.find('\'') {
        if let Some(end) = message[start + 1..].find('\'') {
            return Some(message[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

fn get_line_indent(text: &str, line: usize) -> String {
    text.lines()
        .nth(line)
        .map(|l| {
            let indent_len = l.len() - l.trim_start().len();
            l[..indent_len].to_string()
        })
        .unwrap_or_default()
}

fn compute_folding_ranges(text: &str) -> Vec<FoldingRange> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Failed to load Lua grammar");

    let Some(tree) = parser.parse(text, None) else {
        return vec![];
    };

    let mut ranges = vec![];
    collect_folding_ranges(tree.root_node(), &mut ranges);
    ranges
}

fn collect_folding_ranges(node: tree_sitter::Node, ranges: &mut Vec<FoldingRange>) {
    // Foldable constructs in Lua
    let is_foldable = matches!(
        node.kind(),
        "function_declaration"
            | "function_definition"
            | "if_statement"
            | "for_statement"
            | "while_statement"
            | "repeat_statement"
            | "do_statement"
            | "table_constructor"
    );

    if is_foldable {
        let start = node.start_position();
        let end = node.end_position();

        // Only fold if it spans multiple lines
        if end.row > start.row {
            let kind = match node.kind() {
                "table_constructor" => Some(FoldingRangeKind::Region),
                _ => None,
            };

            ranges.push(FoldingRange {
                start_line: start.row as u32,
                start_character: Some(start.column as u32),
                end_line: end.row as u32,
                end_character: Some(end.column as u32),
                kind,
                collapsed_text: None,
            });
        }
    }

    // Also fold multi-line comments
    if node.kind() == "comment" {
        let start = node.start_position();
        let end = node.end_position();
        if end.row > start.row {
            ranges.push(FoldingRange {
                start_line: start.row as u32,
                start_character: Some(start.column as u32),
                end_line: end.row as u32,
                end_character: Some(end.column as u32),
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: None,
            });
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_folding_ranges(child, ranges);
    }
}

fn format_document(text: &str) -> Option<String> {
    Some(rover_parser::format_code(text))
}

fn compute_semantic_tokens(text: &str, model: &SemanticModel) -> Vec<SemanticToken> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Failed to load Lua grammar");

    let Some(tree) = parser.parse(text, None) else {
        return vec![];
    };

    let mut raw_tokens: Vec<(u32, u32, u32, u32, u32)> = vec![]; // (line, col, len, type, modifiers)
    collect_semantic_tokens(tree.root_node(), text, model, &mut raw_tokens);

    // Sort by position
    raw_tokens.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Convert to delta encoding
    let mut tokens = vec![];
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for (line, col, len, token_type, modifiers) in raw_tokens {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 { col - prev_char } else { col };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        prev_line = line;
        prev_char = col;
    }

    tokens
}

fn collect_semantic_tokens(
    node: tree_sitter::Node,
    text: &str,
    model: &SemanticModel,
    tokens: &mut Vec<(u32, u32, u32, u32, u32)>,
) {
    let start = node.start_position();
    let end = node.end_position();
    let len = if start.row == end.row {
        (end.column - start.column) as u32
    } else {
        (node.end_byte() - node.start_byte()) as u32
    };

    match node.kind() {
        // Keywords
        "local" | "function" | "end" | "if" | "then" | "else" | "elseif" | "for" | "in"
        | "while" | "do" | "repeat" | "until" | "return" | "break" | "and" | "or" | "not"
        | "true" | "false" | "nil" => {
            tokens.push((start.row as u32, start.column as u32, len, 10, 0)); // KEYWORD
        }

        // Strings
        "string" | "string_content" => {
            tokens.push((start.row as u32, start.column as u32, len, 8, 0)); // STRING
        }

        // Numbers
        "number" => {
            tokens.push((start.row as u32, start.column as u32, len, 9, 0)); // NUMBER
        }

        // Comments
        "comment" => {
            tokens.push((start.row as u32, start.column as u32, len, 11, 0)); // COMMENT
        }

        // Identifiers - classify based on context and symbol table
        "identifier" => {
            let name = &text[node.start_byte()..node.end_byte()];
            let (token_type, modifiers) = classify_identifier(node, name, model);
            tokens.push((start.row as u32, start.column as u32, len, token_type, modifiers));
        }

        // Function names in declarations
        "function_name" | "function_name_field" => {
            tokens.push((start.row as u32, start.column as u32, len, 3, 1)); // FUNCTION + DEFINITION
        }

        // Method calls
        "method" => {
            tokens.push((start.row as u32, start.column as u32, len, 4, 0)); // METHOD
        }

        // Table field keys
        "field_name" => {
            tokens.push((start.row as u32, start.column as u32, len, 5, 0)); // PROPERTY
        }

        _ => {}
    }

    // Recurse
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_semantic_tokens(child, text, model, tokens);
    }
}

fn classify_identifier(
    node: tree_sitter::Node,
    name: &str,
    model: &SemanticModel,
) -> (u32, u32) {
    // Check parent context
    let parent = node.parent();

    // Is it a function call?
    if let Some(p) = parent {
        if p.kind() == "function_call" {
            // Check if stdlib
            if is_stdlib_global(name) {
                return (3, 16); // FUNCTION + DEFAULT_LIBRARY
            }
            return (3, 0); // FUNCTION
        }

        // Is it a method call receiver?
        if p.kind() == "method_index_expression" {
            if let Some(first_child) = p.child(0) {
                if first_child.id() == node.id() {
                    // This is the receiver (e.g., "ctx" in "ctx:method()")
                    if model.symbol_specs.contains_key(name) {
                        return (2, 0); // CLASS (Rover type)
                    }
                }
            }
        }

        // Is it being assigned to?
        if p.kind() == "variable_list" {
            if let Some(gp) = p.parent() {
                if gp.kind() == "assignment_statement" || gp.kind() == "variable_declaration" {
                    return (6, 1); // VARIABLE + DEFINITION
                }
            }
        }

        // Is it a parameter?
        if p.kind() == "parameters" {
            return (7, 1); // PARAMETER + DEFINITION
        }
    }

    // Check symbol table
    if let Some(symbol) = model.symbol_table.resolve_symbol_global(name) {
        match symbol.kind {
            rover_parser::SymbolKind::Parameter => return (7, 0), // PARAMETER
            rover_parser::SymbolKind::Function => return (3, 0), // FUNCTION
            rover_parser::SymbolKind::RoverServer | rover_parser::SymbolKind::RoverGuard => {
                return (2, 0) // CLASS
            }
            rover_parser::SymbolKind::ContextParam => return (6, 0), // VARIABLE
            _ => {}
        }
    }

    // Check Rover symbol specs
    if model.symbol_specs.contains_key(name) {
        return (2, 0); // CLASS
    }

    // Stdlib globals
    if is_stdlib_global(name) {
        return (6, 16); // VARIABLE + DEFAULT_LIBRARY
    }

    // Default: regular variable
    (6, 0) // VARIABLE
}

fn is_stdlib_global(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "type"
            | "tostring"
            | "tonumber"
            | "pairs"
            | "ipairs"
            | "next"
            | "pcall"
            | "xpcall"
            | "error"
            | "assert"
            | "require"
            | "select"
            | "unpack"
            | "setmetatable"
            | "getmetatable"
            | "rawget"
            | "rawset"
            | "rawequal"
            | "string"
            | "table"
            | "math"
            | "io"
            | "os"
            | "debug"
            | "coroutine"
            | "package"
            | "collectgarbage"
            | "loadstring"
            | "loadfile"
            | "dofile"
            | "load"
    )
}

fn is_lua_keyword(name: &str) -> bool {
    matches!(
        name,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn compute_document_highlights(text: &str, position: Position) -> Vec<DocumentHighlight> {
    let Some((identifier, _)) = identifier_at_position(text, position) else {
        return vec![];
    };

    let mut highlights = vec![];
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let mut start = 0;
        while let Some(pos) = line[start..].find(&identifier) {
            let col = start + pos;
            let end_col = col + identifier.len();

            // Check word boundaries
            let before_ok =
                col == 0 || !line.as_bytes().get(col - 1).map_or(false, |&b| is_ident_byte(b));
            let after_ok = end_col >= line.len()
                || !line.as_bytes().get(end_col).map_or(false, |&b| is_ident_byte(b));

            if before_ok && after_ok {
                // Determine if this is a write or read
                let kind = if is_write_position(text, line_idx, col) {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                };

                highlights.push(DocumentHighlight {
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
                    kind: Some(kind),
                });
            }

            start = end_col;
        }
    }

    highlights
}

fn is_write_position(text: &str, line: usize, col: usize) -> bool {
    // Simple heuristic: check if followed by '=' but not '=='
    let line_text = text.lines().nth(line).unwrap_or("");
    let after = &line_text[col..];

    // Skip the identifier
    let mut chars = after.chars().peekable();
    while chars.peek().map_or(false, |c| c.is_alphanumeric() || *c == '_') {
        chars.next();
    }

    // Skip whitespace
    while chars.peek().map_or(false, |c| c.is_whitespace()) {
        chars.next();
    }

    // Check for assignment
    if chars.next() == Some('=') {
        return chars.peek() != Some(&'=');
    }

    false
}

fn compute_selection_range(text: &str, position: Position) -> Option<SelectionRange> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Failed to load Lua grammar");

    let tree = parser.parse(text, None)?;

    // Find the smallest node at position
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let mut node = tree.root_node().descendant_for_point_range(point, point)?;

    // Build selection range hierarchy from innermost to outermost
    let mut ranges: Vec<Range> = vec![];

    loop {
        let start = node.start_position();
        let end = node.end_position();

        let range = Range {
            start: Position {
                line: start.row as u32,
                character: start.column as u32,
            },
            end: Position {
                line: end.row as u32,
                character: end.column as u32,
            },
        };

        // Avoid duplicate ranges
        if ranges.last() != Some(&range) {
            ranges.push(range);
        }

        if let Some(parent) = node.parent() {
            node = parent;
        } else {
            break;
        }
    }

    // Convert to nested SelectionRange
    let mut result: Option<SelectionRange> = None;

    for range in ranges.into_iter().rev() {
        result = Some(SelectionRange {
            range,
            parent: result.map(Box::new),
        });
    }

    result
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
