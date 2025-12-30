use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Variable,
    Function,
    Parameter,
    Global,
    Builtin,
    RoverServer,
    RoverGuard,
    ContextParam,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: SourceRange,
    pub type_annotation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeType {
    Global,
    File,
    Function,
    Block,
    Repeat,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub id: usize,
    pub scope_type: ScopeType,
    pub parent: Option<usize>,
    pub symbols: HashMap<String, Symbol>,
    pub range: Option<SourceRange>,  // Range of the scope in source code
}

impl Scope {
    pub fn new(id: usize, scope_type: ScopeType, parent: Option<usize>) -> Self {
        Self {
            id,
            scope_type,
            parent,
            symbols: HashMap::new(),
            range: None,
        }
    }

    pub fn new_with_range(id: usize, scope_type: ScopeType, parent: Option<usize>, range: SourceRange) -> Self {
        Self {
            id,
            scope_type,
            parent,
            symbols: HashMap::new(),
            range: Some(range),
        }
    }

    pub fn set_range(&mut self, range: SourceRange) {
        self.range = Some(range);
    }

    pub fn contains_position(&self, line: usize, column: usize) -> bool {
        match &self.range {
            Some(r) => r.contains(line, column),
            None => true, // Global scope contains everything
        }
    }

    pub fn insert(&mut self, symbol: Symbol) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    current_scope: Option<usize>,
    counter: usize,
}

impl SymbolTable {
    pub fn new() -> Self {
        let global_scope = Scope::new(0, ScopeType::Global, None);
        Self {
            scopes: vec![global_scope],
            current_scope: Some(0),
            counter: 1,
        }
    }

    pub fn push_scope(&mut self, scope_type: ScopeType) {
        let id = self.counter;
        self.counter += 1;
        let parent = self.current_scope;
        let scope = Scope::new(id, scope_type, parent);
        self.scopes.push(scope);
        self.current_scope = Some(id);
    }

    pub fn push_scope_with_range(&mut self, scope_type: ScopeType, range: SourceRange) {
        let id = self.counter;
        self.counter += 1;
        let parent = self.current_scope;
        let scope = Scope::new_with_range(id, scope_type, parent, range);
        self.scopes.push(scope);
        self.current_scope = Some(id);
    }

    pub fn pop_scope(&mut self) {
        if let Some(current) = self.current_scope {
            if let Some(scope) = self.scopes.get(current) {
                if let Some(parent) = scope.parent {
                    self.current_scope = Some(parent);
                } else {
                    self.current_scope = Some(0);
                }
            }
        }
    }

    pub fn insert_symbol(&mut self, symbol: Symbol) {
        if let Some(current) = self.current_scope {
            if let Some(scope) = self.scopes.get_mut(current) {
                scope.insert(symbol);
            }
        }
    }

    pub fn resolve_symbol(&self, name: &str) -> Option<&Symbol> {
        let mut current = self.current_scope?;
        loop {
            if let Some(scope) = self.scopes.get(current) {
                if let Some(symbol) = scope.get(name) {
                    return Some(symbol);
                }
                if let Some(parent) = scope.parent {
                    current = parent;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    }

    pub fn resolve_symbol_from_scope(&self, name: &str, scope_id: usize) -> Option<&Symbol> {
        let mut current = scope_id;
        loop {
            if let Some(scope) = self.scopes.get(current) {
                if let Some(symbol) = scope.get(name) {
                    return Some(symbol);
                }
                if let Some(parent) = scope.parent {
                    current = parent;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    }

    pub fn resolve_symbol_global(&self, name: &str) -> Option<&Symbol> {
        // Search all scopes starting from global (scope 0)
        for scope in &self.scopes {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol);
            }
        }
        None
    }

    pub fn resolve_symbol_at_position(&self, name: &str, line: usize, column: usize) -> Option<&Symbol> {
        // Find innermost scope containing the position
        let scope_id = self.find_innermost_scope_at(line, column)?;
        self.resolve_symbol_from_scope(name, scope_id)
    }

    /// Find the innermost scope that contains the given position
    fn find_innermost_scope_at(&self, line: usize, column: usize) -> Option<usize> {
        let mut best_scope: Option<usize> = None;
        let mut best_depth: usize = 0;

        for scope in &self.scopes {
            if scope.contains_position(line, column) {
                let depth = self.scope_depth(scope.id);
                if depth >= best_depth {
                    best_depth = depth;
                    best_scope = Some(scope.id);
                }
            }
        }

        // Fall back to global scope if nothing found
        best_scope.or(Some(0))
    }

    /// Calculate depth of a scope (distance from global)
    fn scope_depth(&self, scope_id: usize) -> usize {
        let mut depth = 0;
        let mut current = scope_id;
        while let Some(scope) = self.scopes.get(current) {
            if let Some(parent) = scope.parent {
                depth += 1;
                current = parent;
            } else {
                break;
            }
        }
        depth
    }

    pub fn get_current_scope(&self) -> Option<&Scope> {
        self.current_scope
            .and_then(|id| self.scopes.get(id))
    }

    pub fn all_symbols(&self) -> Vec<&Symbol> {
        let mut symbols = Vec::new();
        for scope in &self.scopes {
            for symbol in scope.symbols.values() {
                symbols.push(symbol);
            }
        }
        symbols
    }

    pub fn current_scope_mut(&mut self) -> Option<&mut Scope> {
        self.current_scope
            .and_then(move |id| self.scopes.get_mut(id))
    }

    pub fn get_scope(&self, id: usize) -> Option<&Scope> {
        self.scopes.get(id)
    }

    pub fn get_scope_mut(&mut self, id: usize) -> Option<&mut Scope> {
        self.scopes.get_mut(id)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceRange {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start: SourcePosition { line: start_line, column: start_col },
            end: SourcePosition { line: end_line, column: end_col },
        }
    }

    pub fn contains(&self, line: usize, column: usize) -> bool {
        let after_start = (line > self.start.line) 
            || (line == self.start.line && column >= self.start.column);
        let before_end = (line < self.end.line) 
            || (line == self.end.line && column <= self.end.column);
        after_start && before_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(name: &str, kind: SymbolKind, line: usize, column: usize) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            range: SourceRange {
                start: SourcePosition { line, column },
                end: SourcePosition {
                    line,
                    column: column + name.len(),
                },
            },
            type_annotation: None,
        }
    }

    #[test]
    fn test_scope_insert_and_get() {
        let mut scope = Scope::new(0, ScopeType::Global, None);
        let symbol = make_symbol("x", SymbolKind::Variable, 0, 0);

        scope.insert(symbol.clone());
        assert!(scope.contains("x"));
        assert_eq!(scope.get("x").unwrap().name, "x");
    }

    #[test]
    fn test_symbol_table_push_pop() {
        let mut table = SymbolTable::new();
        assert_eq!(table.get_current_scope().unwrap().id, 0);

        table.push_scope(ScopeType::Function);
        assert_eq!(table.get_current_scope().unwrap().id, 1);
        assert_eq!(table.get_current_scope().unwrap().parent, Some(0));

        table.pop_scope();
        assert_eq!(table.get_current_scope().unwrap().id, 0);
    }

    #[test]
    fn test_symbol_resolution() {
        let mut table = SymbolTable::new();

        let global_x = make_symbol("x", SymbolKind::Global, 0, 0);
        table.insert_symbol(global_x);

        table.push_scope(ScopeType::Function);
        let local_x = make_symbol("x", SymbolKind::Variable, 1, 4);
        table.insert_symbol(local_x);

        let resolved = table.resolve_symbol("x").unwrap();
        assert_eq!(resolved.kind, SymbolKind::Variable);

        table.pop_scope();
        let global_resolved = table.resolve_symbol("x").unwrap();
        assert_eq!(global_resolved.kind, SymbolKind::Global);
    }

    #[test]
    fn test_nested_scopes() {
        let mut table = SymbolTable::new();

        table.push_scope(ScopeType::Function);
        table.push_scope(ScopeType::Block);
        table.push_scope(ScopeType::Repeat);

        let current = table.get_current_scope().unwrap();
        assert_eq!(current.scope_type, ScopeType::Repeat);
        assert_eq!(current.parent, Some(2));

        table.pop_scope();
        assert_eq!(table.get_current_scope().unwrap().scope_type, ScopeType::Block);
    }

    #[test]
    fn test_symbol_shadowing() {
        let mut table = SymbolTable::new();

        let outer = make_symbol("x", SymbolKind::Variable, 0, 0);
        table.insert_symbol(outer);

        table.push_scope(ScopeType::Block);
        let inner = make_symbol("x", SymbolKind::Variable, 1, 4);
        table.insert_symbol(inner);

        let resolved = table.resolve_symbol("x").unwrap();
        assert_eq!(resolved.range.start.line, 1);
    }

    #[test]
    fn test_resolve_symbol_at_position() {
        let mut table = SymbolTable::new();

        // Global scope (no range = contains everything)
        let global_x = make_symbol("x", SymbolKind::Global, 0, 0);
        table.insert_symbol(global_x);

        // Function scope at lines 2-10
        let func_range = SourceRange::new(2, 0, 10, 3);
        table.push_scope_with_range(ScopeType::Function, func_range);
        let func_x = make_symbol("x", SymbolKind::Variable, 3, 4);
        table.insert_symbol(func_x);

        // Inner block at lines 5-8
        let block_range = SourceRange::new(5, 0, 8, 3);
        table.push_scope_with_range(ScopeType::Block, block_range);
        let block_x = make_symbol("x", SymbolKind::Variable, 6, 8);
        table.insert_symbol(block_x);

        // Query at line 6 (inside block) - should find block_x
        let resolved_in_block = table.resolve_symbol_at_position("x", 6, 10).unwrap();
        assert_eq!(resolved_in_block.range.start.line, 6);

        // Query at line 4 (inside function, outside block) - should find func_x
        let resolved_in_func = table.resolve_symbol_at_position("x", 4, 0).unwrap();
        assert_eq!(resolved_in_func.range.start.line, 3);

        // Query at line 15 (outside function) - should find global_x
        let resolved_global = table.resolve_symbol_at_position("x", 15, 0).unwrap();
        assert_eq!(resolved_global.range.start.line, 0);
    }

    #[test]
    fn test_source_range_contains() {
        let range = SourceRange::new(5, 10, 10, 20);

        // Inside
        assert!(range.contains(7, 0));
        assert!(range.contains(5, 15));
        assert!(range.contains(10, 10));

        // At boundaries
        assert!(range.contains(5, 10));
        assert!(range.contains(10, 20));

        // Outside
        assert!(!range.contains(4, 0));
        assert!(!range.contains(11, 0));
        assert!(!range.contains(5, 5));
        assert!(!range.contains(10, 25));
    }
}
