use std::collections::HashMap;
use std::sync::OnceLock;

mod data;

use data::build_specs;

static SPEC_REGISTRY: OnceLock<ApiSpecRegistry> = OnceLock::new();

fn registry() -> &'static ApiSpecRegistry {
    SPEC_REGISTRY.get_or_init(|| ApiSpecRegistry::new(build_specs()))
}

pub fn lookup_spec(id: &str) -> Option<SpecDoc> {
    registry().doc(id)
}

struct ApiSpecRegistry {
    specs: Vec<ApiSpec>,
    index: HashMap<&'static str, usize>,
}

impl ApiSpecRegistry {
    fn new(specs: Vec<ApiSpec>) -> Self {
        let mut index = HashMap::new();
        for (idx, spec) in specs.iter().enumerate() {
            index.insert(spec.id, idx);
        }
        Self { specs, index }
    }

    fn get(&self, id: &str) -> Option<&ApiSpec> {
        self.index.get(id).and_then(|idx| self.specs.get(*idx))
    }

    fn doc(&self, id: &str) -> Option<SpecDoc> {
        self.get(id).map(SpecDoc::from)
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub(super) struct ApiSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub doc: &'static str,
    pub kind: SpecKind,
    pub params: Vec<ApiParam>,
    pub returns: Option<&'static str>,
    pub members: Vec<ApiMember>,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(super) enum SpecKind {
    Object,
    Function,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Field,
    Method,
}

#[derive(Clone)]
pub(super) struct ApiMember {
    pub name: &'static str,
    pub target: &'static str,
    pub doc: &'static str,
    pub kind: MemberKind,
}

#[allow(dead_code)]
#[derive(Clone)]
pub(super) struct ApiParam {
    pub name: &'static str,
    pub type_name: &'static str,
    pub doc: &'static str,
}

#[derive(Clone)]
pub struct SpecDoc {
    pub id: &'static str,
    pub doc: &'static str,
    pub members: Vec<SpecDocMember>,
}

#[derive(Clone)]
pub struct SpecDocMember {
    pub name: &'static str,
    pub doc: &'static str,
    pub target: &'static str,
    pub kind: MemberKind,
}

impl From<&ApiSpec> for SpecDoc {
    fn from(spec: &ApiSpec) -> Self {
        SpecDoc {
            id: spec.id,
            doc: spec.doc,
            members: spec.members.iter().map(SpecDocMember::from).collect(),
        }
    }
}

impl From<&ApiMember> for SpecDocMember {
    fn from(member: &ApiMember) -> Self {
        SpecDocMember {
            name: member.name,
            doc: member.doc,
            target: member.target,
            kind: member.kind,
        }
    }
}
