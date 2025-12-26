use std::sync::OnceLock;

use crate::analyzer::Analyzer;
use crate::rule_runtime::{
    ApiMember, ApiParam, ApiSpec, Rule, RuleEngine, RuleEngineBuilder, Selector, SpecKind,
};

macro_rules! selector {
    (node $kind:literal { $($body:tt)* }) => {{
        let mut sel = Selector::node($kind);
        selector!(@body sel { $($body)* });
        sel
    }};
    (node $kind:literal) => {
        Selector::node($kind)
    };
    (alias $name:literal { $($body:tt)* }) => {{
        let mut sel = Selector::alias($name);
        selector!(@body sel { $($body)* });
        sel
    }};
    (alias $name:literal) => {
        Selector::alias($name)
    };
    (@body $sel:ident { $( $stmt:tt; )* }) => {
        $( selector!(@stmt $sel $stmt); )*
    };
    (@stmt $sel:ident capture $name:ident) => {
        $sel = $sel.capture(stringify!($name));
    };
    (@stmt $sel:ident has { $($inner:tt)* }) => {
        let inner = selector! { $($inner)* };
        $sel = $sel.has(inner);
    };
    (@stmt $sel:ident descendant { $($inner:tt)* }) => {
        let inner = selector! { $($inner)* };
        $sel = $sel.descendant(inner);
    };
    (@stmt $sel:ident ancestor { $($inner:tt)* }) => {
        let inner = selector! { $($inner)* };
        $sel = $sel.ancestor(inner);
    };
    (@stmt $sel:ident method $name:literal) => {
        $sel = $sel.method($name);
    };
    (@stmt $sel:ident callee $path:literal) => {
        $sel = $sel.callee($path);
    };
}

macro_rules! rule {
    (
        name: $name:ident,
        selector: $selector:expr,
        $(enter: $enter:expr,)?
        $(exit: $exit:expr,)?
    ) => {
        Rule::new(
            stringify!($name),
            $selector,
            rule!(@opt $($enter)?),
            rule!(@opt $($exit)?),
        )
    };
    (@opt) => {
        None
    };
    (@opt $action:expr) => {
        Some($action)
    };
}

macro_rules! api_member {
    ($name:literal => $target:literal, $doc:literal) => {
        ApiMember {
            name: $name,
            target: $target,
            doc: $doc,
        }
    };
}

macro_rules! api_param {
    ($name:literal, $ty:literal, $doc:literal) => {
        ApiParam {
            name: $name,
            type_name: $ty,
            doc: $doc,
        }
    };
}

macro_rules! api_object {
    ($id:literal, $doc:literal, [ $( $member:expr ),* ]) => {
        ApiSpec {
            id: $id,
            name: $id,
            doc: $doc,
            kind: SpecKind::Object,
            params: Vec::new(),
            returns: None,
            members: vec![ $( $member ),* ],
        }
    };
}

macro_rules! api_function {
    ($id:literal, $doc:literal, [ $( $param:expr ),* ], $returns:expr) => {
        ApiSpec {
            id: $id,
            name: $id,
            doc: $doc,
            kind: SpecKind::Function,
            params: vec![ $( $param ),* ],
            returns: $returns,
            members: Vec::new(),
        }
    };
}

static SPEC_STORE: OnceLock<Vec<ApiSpec>> = OnceLock::new();

fn spec_catalog() -> &'static Vec<ApiSpec> {
    SPEC_STORE.get_or_init(build_specs)
}

pub fn lookup_spec(id: &str) -> Option<&'static ApiSpec> {
    spec_catalog().iter().find(|spec| spec.id == id)
}

fn build_specs() -> Vec<ApiSpec> {
    vec![
        api_object!(
            "rover",
            "Global Rover namespace.",
            [
                api_member!("server" => "rover_server", "Create a Rover server."),
                api_member!("guard" => "rover_guard", "Guard builder namespace."),
                api_member!("json" => "rover_response_json", "Return JSON response."),
                api_member!("text" => "rover_response_text", "Return text response."),
                api_member!("html" => "rover_response_html", "Return HTML response."),
                api_member!("error" => "rover_response_error", "Return error response.")
            ]
        ),
        api_object!("rover_server", "Rover server instance with route definitions.", []),
        api_object!(
            "rover_guard",
            "Guard helper namespace.",
            [
                api_member!("string" => "rover_guard_string", "String guard."),
                api_member!("integer" => "rover_guard_integer", "Integer guard."),
                api_member!("number" => "rover_guard_number", "Number guard."),
                api_member!("boolean" => "rover_guard_boolean", "Boolean guard."),
                api_member!("array" => "rover_guard_array", "Array guard."),
                api_member!("object" => "rover_guard_object", "Object guard.")
            ]
        ),
        api_function!("rover_guard_string", "Create string guard.", [], Some("Guard<String>")),
        api_function!("rover_guard_integer", "Create integer guard.", [], Some("Guard<Integer>")),
        api_function!("rover_guard_number", "Create number guard.", [], Some("Guard<Number>")),
        api_function!("rover_guard_boolean", "Create boolean guard.", [], Some("Guard<Boolean>")),
        api_function!(
            "rover_guard_array",
            "Create array guard.",
            [api_param!("inner", "Guard", "Inner guard")],
            Some("Guard<Array>")
        ),
        api_function!(
            "rover_guard_object",
            "Create object guard.",
            [api_param!("shape", "GuardShape", "Object shape")],
            Some("Guard<Object>")
        ),
        api_function!("rover_response_json", "Build JSON response.", [], Some("RoverResponse")),
        api_function!("rover_response_text", "Build text response.", [], Some("RoverResponse")),
        api_function!("rover_response_html", "Build HTML response.", [], Some("RoverResponse")),
        api_function!("rover_response_error", "Build error response.", [], Some("RoverResponse")),
        api_object!(
            "ctx",
            "Handler context parameter.",
            [
                api_member!("params" => "ctx_params", "Access path params."),
                api_member!("query" => "ctx_query", "Access query params."),
                api_member!("headers" => "ctx_headers", "Access headers."),
                api_member!("body" => "ctx_body", "Access body handle.")
            ]
        ),
        api_object!("ctx_params", "Path params accessor.", []),
        api_object!("ctx_query", "Query params accessor.", []),
        api_object!("ctx_headers", "Header accessor.", []),
        api_object!(
            "ctx_body",
            "Body accessor with expect().",
            [api_member!("expect" => "ctx_body_expect", "Validate body with guards.")]
        ),
        api_function!(
            "ctx_body_expect",
            "Expect body schema.",
            [api_param!("schema", "GuardShape", "Body schema")],
            Some("GuardBinding")
        ),
        api_object!("lua_string", "Lua string library.", []),
    ]
}

pub fn build_rule_engine() -> RuleEngine<Analyzer> {
    let specs = spec_catalog().clone();

    let mut builder: RuleEngineBuilder<Analyzer> = RuleEngineBuilder::new().with_specs(specs);


    builder.push_alias("handler_fn", selector! { node "function_declaration" });

    builder.push_rule(rule! {
        name: server_assignment,
        selector: selector! { node "assignment_statement" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.process_assignment(node);
        },
    });

    builder.push_rule(rule! {
        name: handler_function,
        selector: selector! { alias "handler_fn" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.enter_handler_function(node);
        },
        exit: |ctx: &mut Analyzer, _node, _| {
            ctx.exit_handler_function();
        },
    });

    builder.push_rule(rule! {
        name: return_statement,
        selector: selector! { node "return_statement" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_return(node);
        },
    });

    builder.push_rule(rule! {
        name: function_call,
        selector: selector! { node "function_call" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.process_function_call(node);
        },
    });

    builder.build()
}
