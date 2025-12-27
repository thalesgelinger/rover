use std::sync::OnceLock;

use crate::analyzer::Analyzer;
use crate::rule_runtime::{
    ApiMember, ApiParam, ApiSpec, MemberKind, Rule, RuleEngine, RuleEngineBuilder, Selector, SpecDoc, SpecKind,
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
    // Empty body - base case
    (@body $sel:ident { }) => {};
    // capture statement
    (@body $sel:ident { capture $name:ident ; $($rest:tt)* }) => {
        $sel = $sel.capture(stringify!($name));
        selector!(@body $sel { $($rest)* });
    };
    // has statement with nested selector
    (@body $sel:ident { has { $($inner:tt)* } ; $($rest:tt)* }) => {
        $sel = $sel.has(selector!{ $($inner)* });
        selector!(@body $sel { $($rest)* });
    };
    // descendant statement with nested selector
    (@body $sel:ident { descendant { $($inner:tt)* } ; $($rest:tt)* }) => {
        $sel = $sel.descendant(selector!{ $($inner)* });
        selector!(@body $sel { $($rest)* });
    };
    // ancestor statement with nested selector
    (@body $sel:ident { ancestor { $($inner:tt)* } ; $($rest:tt)* }) => {
        $sel = $sel.ancestor(selector!{ $($inner)* });
        selector!(@body $sel { $($rest)* });
    };
    // method statement
    (@body $sel:ident { method $name:literal ; $($rest:tt)* }) => {
        $sel = $sel.method($name);
        selector!(@body $sel { $($rest)* });
    };
    // callee statement
    (@body $sel:ident { callee $path:literal ; $($rest:tt)* }) => {
        $sel = $sel.callee($path);
        selector!(@body $sel { $($rest)* });
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
    ($name:literal => $target:literal, $doc:literal, field) => {
        ApiMember {
            name: $name,
            target: $target,
            doc: $doc,
            kind: MemberKind::Field,
        }
    };
    ($name:literal => $target:literal, $doc:literal, method) => {
        ApiMember {
            name: $name,
            target: $target,
            doc: $doc,
            kind: MemberKind::Method,
        }
    };
    // Default to method for backward compatibility
    ($name:literal => $target:literal, $doc:literal) => {
        ApiMember {
            name: $name,
            target: $target,
            doc: $doc,
            kind: MemberKind::Method,
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

static SPEC_REGISTRY: OnceLock<crate::rule_runtime::ApiSpecRegistry> = OnceLock::new();

fn spec_registry() -> &'static crate::rule_runtime::ApiSpecRegistry {
    SPEC_REGISTRY.get_or_init(|| crate::rule_runtime::ApiSpecRegistry::new(build_specs()))
}

pub fn lookup_spec(id: &str) -> Option<SpecDoc> {
    spec_registry().doc(id)
}

fn build_specs() -> Vec<ApiSpec> {
    vec![
        api_object!(
            "rover",
            "Global Rover namespace.",
            [
                api_member!("server" => "rover_server_constructor", "Create a Rover server.", method),
                api_member!("guard" => "rover_guard", "Guard builder namespace.", field)
            ]
        ),
        api_function!(
            "rover_server_constructor", 
            "Create a Rover server instance. Pass config table with host, port, log_level.", 
            [api_param!("config", "ServerConfig", "Server configuration table")], 
            Some("RoverServer")
        ),
        api_object!(
            "rover_server_config",
            "Server configuration table.",
            [
                api_member!("host" => "string", "Server host (default: 127.0.0.1)", field),
                api_member!("port" => "number", "Server port (default: 4242)", field),
                api_member!("log_level" => "string", "Log level: debug, info, warn, error, nope", field)
            ]
        ),
        api_object!(
            "rover_server", 
            "Rover server instance with route definitions and response builders.", 
            [
                api_member!("json" => "rover_response_json", "Return JSON response.", method),
                api_member!("text" => "rover_response_text", "Return text response.", method),
                api_member!("html" => "rover_response_html", "Return HTML response.", method),
                api_member!("error" => "rover_response_error", "Return error response.", method),
                api_member!("redirect" => "rover_response_redirect", "Return redirect response.", method),
                api_member!("no_content" => "rover_response_no_content", "Return 204 No Content response.", method)
            ]
        ),
        api_object!(
            "rover_guard",
            "Guard helper namespace.",
            [
                api_member!("string" => "rover_guard_string", "String guard.", method),
                api_member!("integer" => "rover_guard_integer", "Integer guard.", method),
                api_member!("number" => "rover_guard_number", "Number guard.", method),
                api_member!("boolean" => "rover_guard_boolean", "Boolean guard.", method),
                api_member!("array" => "rover_guard_array", "Array guard.", method),
                api_member!("object" => "rover_guard_object", "Object guard.", method)
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
        api_function!("rover_response_json", "Build JSON response. Can chain :status(code, data).", [], Some("RoverResponse")),
        api_function!("rover_response_text", "Build text response. Can chain :status(code, text).", [], Some("RoverResponse")),
        api_function!("rover_response_html", "Build HTML response. Can chain :status(code, html).", [], Some("RoverResponse")),
        api_function!("rover_response_error", "Build error response with status code and message.", [], Some("RoverResponse")),
        api_function!("rover_response_redirect", "Build redirect response. Can chain :permanent() or :status().", [], Some("RoverResponse")),
        api_function!("rover_response_no_content", "Build 204 No Content response.", [], Some("RoverResponse")),
        api_object!(
            "ctx",
            "Handler context parameter.",
            [
                api_member!("method" => "string", "HTTP method (GET, POST, etc.)", field),
                api_member!("path" => "string", "Request path", field),
                api_member!("params" => "ctx_params", "Access path params.", method),
                api_member!("query" => "ctx_query", "Access query params.", method),
                api_member!("headers" => "ctx_headers", "Access headers.", method),
                api_member!("body" => "ctx_body", "Access body handle.", method)
            ]
        ),
        api_object!("ctx_params", "Path params accessor.", []),
        api_object!("ctx_query", "Query params accessor.", []),
        api_object!("ctx_headers", "Header accessor.", []),
        api_object!(
            "ctx_body",
            "Body accessor with expect().",
            [api_member!("expect" => "ctx_body_expect", "Validate body with guards.", method)]
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
    let specs = spec_registry().all().clone();

    let mut builder: RuleEngineBuilder<Analyzer> = RuleEngineBuilder::new().with_specs(specs);

    // Aliases for common patterns
    builder.push_alias("handler_fn", selector! { node "function_declaration" });
    builder.push_alias(
        "rover_server_call",
        selector! { node "function_call" { callee "rover.server"; } },
    );
    builder.push_alias(
        "rover_guard_call",
        selector! { node "function_call" { callee "rover.guard"; } },
    );
    builder.push_alias(
        "rover_guard_ref",
        selector! { node "dot_index_expression" { callee "rover.guard"; } },
    );

    // Rule: detect `local x = rover.server{}` (local declaration)
    builder.push_rule(rule! {
        name: rover_server_local,
        selector: selector! { node "variable_declaration" {
            descendant { alias "rover_server_call" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_rover_server_assignment(node);
        },
    });

    // Rule: detect `x = rover.server{}` (assignment)
    builder.push_rule(rule! {
        name: rover_server_assignment,
        selector: selector! { node "assignment_statement" {
            descendant { alias "rover_server_call" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_rover_server_assignment(node);
        },
    });

    // Rule: detect `local g = rover.guard` (direct reference)
    builder.push_rule(rule! {
        name: rover_guard_local_direct,
        selector: selector! { node "variable_declaration" {
            descendant { node "dot_index_expression" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_potential_guard_assignment(node);
        },
    });

    // Rule: detect `g = rover.guard` (direct reference assignment)
    builder.push_rule(rule! {
        name: rover_guard_assignment_direct,
        selector: selector! { node "assignment_statement" {
            descendant { node "dot_index_expression" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_potential_guard_assignment(node);
        },
    });

    // Rule: detect `local g = rover.guard()` (function call)
    builder.push_rule(rule! {
        name: rover_guard_local,
        selector: selector! { node "variable_declaration" {
            descendant { alias "rover_guard_call" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_rover_guard_assignment(node);
        },
    });

    // Rule: detect `g = rover.guard()` (function call assignment)
    builder.push_rule(rule! {
        name: rover_guard_assignment,
        selector: selector! { node "assignment_statement" {
            descendant { alias "rover_guard_call" };
        } },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_rover_guard_assignment(node);
        },
    });

    // Rule: handler function (first param gets ctx spec)
    builder.push_rule(rule! {
        name: handler_function,
        selector: selector! { alias "handler_fn" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.track_function_assignment(node);
            ctx.enter_handler_function(node);
        },
        exit: |ctx: &mut Analyzer, _node, _| {
            ctx.exit_handler_function();
        },
    });

    // Rule: return statements in handlers
    builder.push_rule(rule! {
        name: return_statement,
        selector: selector! { node "return_statement" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.handle_return(node);
        },
    });

    // Rule: function calls (for ctx method tracking, guards, etc.)
    builder.push_rule(rule! {
        name: function_call,
        selector: selector! { node "function_call" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.process_function_call(node);
        },
    });

    // Rule: validate dot access (rover.something)
    builder.push_rule(rule! {
        name: validate_dot_access,
        selector: selector! { node "dot_index_expression" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.validate_member_access(node);
        },
    });

    // Rule: validate method access (g:something())
    builder.push_rule(rule! {
        name: validate_method_access,
        selector: selector! { node "method_index_expression" },
        enter: |ctx: &mut Analyzer, node, _| {
            ctx.validate_member_access(node);
        },
    });

    builder.build()
}
