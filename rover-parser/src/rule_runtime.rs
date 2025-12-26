use std::collections::HashMap;

use tree_sitter::Node;

pub trait RuleContext {
    fn source(&self) -> &str;
    fn method_name(&self, node: Node) -> Option<String>;
    fn callee_path(&self, node: Node) -> Option<String>;
}

pub struct Rule<C> {
    pub name: &'static str,
    pub selector: Selector,
    pub enter: Option<RuleAction<C>>,
    pub exit: Option<RuleAction<C>>,
}

impl<C> Rule<C> {
    pub fn new(
        name: &'static str,
        selector: Selector,
        enter: Option<RuleAction<C>>,
        exit: Option<RuleAction<C>>,
    ) -> Self {
        Self {
            name,
            selector,
            enter,
            exit,
        }
    }
}

pub type RuleAction<C> = for<'tree> fn(&mut C, Node<'tree>, &CaptureMap<'tree>);

pub struct RuleEngine<C> {
    aliases: HashMap<&'static str, Selector>,
    rules: Vec<Rule<C>>,
    specs: ApiSpecRegistry,
}

impl<C> RuleEngine<C> {
    pub fn new(
        aliases: HashMap<&'static str, Selector>,
        rules: Vec<Rule<C>>,
        specs: ApiSpecRegistry,
    ) -> Self {
        Self {
            aliases,
            rules,
            specs,
        }
    }

    pub fn specs(&self) -> &ApiSpecRegistry {
        &self.specs
    }

    pub fn apply<'tree>(&self, ctx: &mut C, node: Node<'tree>) -> Vec<ActiveMatch<'tree>>
    where
        C: RuleContext,
    {
        let mut actives = Vec::new();
        for (idx, rule) in self.rules.iter().enumerate() {
            let mut captures = CaptureMap::default();
            if rule
                .selector
                .matches(ctx, self, node, &mut captures)
            {
                if let Some(enter) = rule.enter {
                    enter(ctx, node, &captures);
                }
                if rule.exit.is_some() {
                    actives.push(ActiveMatch {
                        rule_index: idx,
                        captures,
                    });
                }
            }
        }
        actives
    }

    pub fn finish<'tree>(&self, ctx: &mut C, node: Node<'tree>, actives: Vec<ActiveMatch<'tree>>) {
        for active in actives {
            if let Some(rule) = self.rules.get(active.rule_index) {
                if let Some(exit) = rule.exit {
                    exit(ctx, node, &active.captures);
                }
            }
        }
    }
}

pub struct ActiveMatch<'tree> {
    pub rule_index: usize,
    pub captures: CaptureMap<'tree>,
}

#[derive(Default)]
pub struct CaptureMap<'tree> {
    entries: HashMap<&'static str, Node<'tree>>,
}

impl<'tree> CaptureMap<'tree> {
    pub fn insert(&mut self, name: &'static str, node: Node<'tree>) {
        self.entries.insert(name, node);
    }

    pub fn get(&self, name: &str) -> Option<Node<'tree>> {
        self.entries.get(name).copied()
    }

    pub fn merge(&mut self, other: CaptureMap<'tree>) {
        for (key, value) in other.entries {
            self.entries.insert(key, value);
        }
    }
}

#[derive(Clone)]
pub struct Selector {
    target: SelectorTarget,
    capture: Option<&'static str>,
    filters: Vec<SelectorFilter>,
}

impl Selector {
    pub fn node(kind: &'static str) -> Self {
        Self {
            target: SelectorTarget::Node(kind),
            capture: None,
            filters: Vec::new(),
        }
    }

    pub fn alias(name: &'static str) -> Self {
        Self {
            target: SelectorTarget::Alias(name),
            capture: None,
            filters: Vec::new(),
        }
    }

    pub fn capture(mut self, name: &'static str) -> Self {
        self.capture = Some(name);
        self
    }

    pub fn has(mut self, selector: Selector) -> Self {
        self.filters.push(SelectorFilter::Child(Box::new(selector)));
        self
    }

    pub fn descendant(mut self, selector: Selector) -> Self {
        self.filters
            .push(SelectorFilter::Descendant(Box::new(selector)));
        self
    }

    pub fn ancestor(mut self, selector: Selector) -> Self {
        self.filters
            .push(SelectorFilter::Ancestor(Box::new(selector)));
        self
    }

    pub fn method(mut self, name: &'static str) -> Self {
        self.filters
            .push(SelectorFilter::Method(name));
        self
    }

    pub fn callee(mut self, path: &'static str) -> Self {
        self.filters
            .push(SelectorFilter::Callee(path));
        self
    }

    fn matches<'tree, C: RuleContext>(
        &self,
        ctx: &C,
        engine: &RuleEngine<C>,
        node: Node<'tree>,
        captures: &mut CaptureMap<'tree>,
    ) -> bool {
        if !self.target_matches(ctx, engine, node, captures) {
            return false;
        }

        if let Some(name) = self.capture {
            captures.insert(name, node);
        }

        for filter in &self.filters {
            if !filter.matches(ctx, engine, node, captures) {
                return false;
            }
        }

        true
    }

    fn target_matches<'tree, C: RuleContext>(
        &self,
        ctx: &C,
        engine: &RuleEngine<C>,
        node: Node<'tree>,
        captures: &mut CaptureMap<'tree>,
    ) -> bool {
        match &self.target {
            SelectorTarget::Node(kind) => node.kind() == *kind,
            SelectorTarget::Alias(name) => {
                if let Some(alias) = engine.aliases.get(name) {
                    let mut alias_captures = CaptureMap::default();
                    let matched = alias.matches(ctx, engine, node, &mut alias_captures);
                    if matched {
                        captures.merge(alias_captures);
                    }
                    matched
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Clone)]
enum SelectorTarget {
    Node(&'static str),
    Alias(&'static str),
}

#[derive(Clone)]
enum SelectorFilter {
    Child(Box<Selector>),
    Descendant(Box<Selector>),
    Ancestor(Box<Selector>),
    Method(&'static str),
    Callee(&'static str),
}

impl SelectorFilter {
    fn matches<'tree, C: RuleContext>(
        &self,
        ctx: &C,
        engine: &RuleEngine<C>,
        node: Node<'tree>,
        captures: &mut CaptureMap<'tree>,
    ) -> bool {
        match self {
            SelectorFilter::Child(selector) => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    let mut child_caps = CaptureMap::default();
                    if selector.matches(ctx, engine, child, &mut child_caps) {
                        captures.merge(child_caps);
                        return true;
                    }
                }
                false
            }
            SelectorFilter::Descendant(selector) => {
                Self::match_descendant(ctx, engine, node, selector, captures)
            }
            SelectorFilter::Ancestor(selector) => {
                let mut current = node.parent();
                while let Some(parent) = current {
                    let mut ancestor_caps = CaptureMap::default();
                    if selector.matches(ctx, engine, parent, &mut ancestor_caps) {
                        captures.merge(ancestor_caps);
                        return true;
                    }
                    current = parent.parent();
                }
                false
            }
            SelectorFilter::Method(name) => {
                if let Some(method_name) = ctx.method_name(node) {
                    method_name == *name
                } else {
                    false
                }
            }
            SelectorFilter::Callee(path) => {
                if let Some(value) = ctx.callee_path(node) {
                    value == *path
                } else {
                    false
                }
            }
        }
    }

    fn match_descendant<'tree, C: RuleContext>(
        ctx: &C,
        engine: &RuleEngine<C>,
        node: Node<'tree>,
        selector: &Selector,
        captures: &mut CaptureMap<'tree>,
    ) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let mut child_caps = CaptureMap::default();
            if selector.matches(ctx, engine, child, &mut child_caps) {
                captures.merge(child_caps);
                return true;
            }
            if Self::match_descendant(ctx, engine, child, selector, captures) {
                return true;
            }
        }
        false
    }
}

pub struct RuleEngineBuilder<C> {
    aliases: Vec<(&'static str, Selector)>,
    rules: Vec<Rule<C>>,
    specs: Vec<ApiSpec>,
}

impl<C> RuleEngineBuilder<C> {
    pub fn new() -> Self {
        Self {
            aliases: Vec::new(),
            rules: Vec::new(),
            specs: Vec::new(),
        }
    }

    pub fn alias(mut self, name: &'static str, selector: Selector) -> Self {
        self.aliases.push((name, selector));
        self
    }

    pub fn push_alias(&mut self, name: &'static str, selector: Selector) {
        self.aliases.push((name, selector));
    }

    pub fn push_rule(&mut self, rule: Rule<C>) {
        self.rules.push(rule);
    }

    pub fn with_specs(mut self, specs: Vec<ApiSpec>) -> Self {
        self.specs = specs;
        self
    }

    pub fn build(self) -> RuleEngine<C> {
        let alias_map = self.aliases.into_iter().collect();
        RuleEngine::new(alias_map, self.rules, ApiSpecRegistry::new(self.specs))
    }
}

pub struct ApiSpecRegistry {
    specs: Vec<ApiSpec>,
    index: HashMap<&'static str, usize>,
}

impl ApiSpecRegistry {
    pub fn new(specs: Vec<ApiSpec>) -> Self {
        let mut index = HashMap::new();
        for (idx, spec) in specs.iter().enumerate() {
            index.insert(spec.id, idx);
        }
        Self { specs, index }
    }

    pub fn get(&self, id: &str) -> Option<&ApiSpec> {
        self.index
            .get(id)
            .and_then(|idx| self.specs.get(*idx))
    }

    pub fn all(&self) -> &Vec<ApiSpec> {
        &self.specs
    }
}

#[derive(Clone)]
pub struct ApiSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub doc: &'static str,
    pub kind: SpecKind,
    pub params: Vec<ApiParam>,
    pub returns: Option<&'static str>,
    pub members: Vec<ApiMember>,
}

#[derive(Clone, Copy)]
pub enum SpecKind {
    Object,
    Function,
}

#[derive(Clone)]
pub struct ApiMember {
    pub name: &'static str,
    pub target: &'static str,
    pub doc: &'static str,
}

#[derive(Clone)]
pub struct ApiParam {
    pub name: &'static str,
    pub type_name: &'static str,
    pub doc: &'static str,
}
