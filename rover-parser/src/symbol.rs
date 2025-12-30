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
}

impl Scope {
    pub fn new(id: usize, scope_type: ScopeType, parent: Option<usize>) -> Self {
        Self {
            id,
            scope_type,
            parent,
            symbols: HashMap::new(),
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

    pub fn get_current_scope(&self) -> Option<&Scope> {
        self.current_scope
            .and_then(|id| self.scopes.get(id))
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
}
