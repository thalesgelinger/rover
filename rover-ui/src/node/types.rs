use crate::signal::{DerivedId, SignalId};
use mlua::RegistryKey;
use smartstring::{LazyCompact, SmartString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub(crate) u32);

#[derive(Clone)]
pub enum TextContent {
    Static(SmartString<LazyCompact>),
    Signal(SignalId),
    Derived(DerivedId),
}

pub struct TextNode {
    pub content: TextContent,
    pub style: Option<TextStyle>,
}

#[derive(Debug, Clone, Default)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
}

pub struct ContainerNode {
    pub children: Vec<NodeId>,
}

pub struct ConditionalNode {
    pub condition_signal: SignalId,
    pub true_branch: Option<NodeId>,
    pub false_branch: Option<NodeId>,
    pub visible: bool,
}

pub struct EachNode {
    pub list_signal: SignalId,
    pub render_fn_key: Option<RegistryKey>,
    pub children: Vec<(SmartString<LazyCompact>, NodeId)>,
}

pub enum Node {
    Text(TextNode),
    Column(ContainerNode),
    Row(ContainerNode),
    Conditional(ConditionalNode),
    Each(EachNode),
}

impl Node {
    pub fn text(content: TextContent) -> Self {
        Self::Text(TextNode {
            content,
            style: None,
        })
    }

    pub fn column() -> Self {
        Self::Column(ContainerNode { children: vec![] })
    }

    pub fn row() -> Self {
        Self::Row(ContainerNode { children: vec![] })
    }

    pub fn conditional(condition: SignalId) -> Self {
        Self::Conditional(ConditionalNode {
            condition_signal: condition,
            true_branch: None,
            false_branch: None,
            visible: false,
        })
    }

    pub fn each(list: SignalId) -> Self {
        Self::Each(EachNode {
            list_signal: list,
            render_fn_key: None,
            children: vec![],
        })
    }
}
