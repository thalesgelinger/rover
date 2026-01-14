use std::sync::OnceLock;

use crate::analyzer::Analyzer;
use crate::rule_runtime::{
    ApiMember, ApiParam, ApiSpec, MemberKind, Rule, RuleEngine, RuleEngineBuilder, Selector,
    SpecDoc, SpecKind,
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
        // Lua 5.1 Global Functions
        api_function!(
            "print",
            "Print values to stdout.",
            [api_param!("...", "any", "Values to print")],
            None
        ),
        api_function!(
            "assert",
            "Assert condition is true.",
            [
                api_param!("condition", "boolean", "Condition"),
                api_param!("message", "string", "Error message")
            ],
            None
        ),
        api_function!(
            "error",
            "Raise error.",
            [
                api_param!("message", "string", "Error message"),
                api_param!("level", "number", "Error level")
            ],
            None
        ),
        api_function!(
            "ipairs",
            "Iterator for arrays.",
            [api_param!("t", "table", "Table to iterate")],
            Some("function")
        ),
        api_function!(
            "pairs",
            "Iterator for tables.",
            [api_param!("t", "table", "Table to iterate")],
            Some("function")
        ),
        api_function!(
            "next",
            "Next key/value in table.",
            [
                api_param!("t", "table", "Table"),
                api_param!("index", "any", "Current index")
            ],
            Some("any")
        ),
        api_function!(
            "pcall",
            "Protected call.",
            [
                api_param!("f", "function", "Function to call"),
                api_param!("...", "any", "Arguments")
            ],
            Some("boolean")
        ),
        api_function!(
            "xpcall",
            "Extended protected call.",
            [
                api_param!("f", "function", "Function"),
                api_param!("err", "function", "Error handler")
            ],
            Some("boolean")
        ),
        api_function!(
            "select",
            "Select arguments.",
            [
                api_param!("index", "any", "Index"),
                api_param!("...", "any", "Arguments")
            ],
            Some("any")
        ),
        api_function!(
            "tonumber",
            "Convert to number.",
            [
                api_param!("e", "any", "Value"),
                api_param!("base", "number", "Base")
            ],
            Some("number")
        ),
        api_function!(
            "tostring",
            "Convert to string.",
            [api_param!("v", "any", "Value")],
            Some("string")
        ),
        api_function!(
            "type",
            "Get type of value.",
            [api_param!("v", "any", "Value")],
            Some("string")
        ),
        api_function!(
            "getmetatable",
            "Get metatable.",
            [api_param!("object", "any", "Object")],
            Some("table")
        ),
        api_function!(
            "setmetatable",
            "Set metatable.",
            [
                api_param!("table", "table", "Table"),
                api_param!("metatable", "table", "Metatable")
            ],
            Some("table")
        ),
        api_function!(
            "rawget",
            "Raw table get.",
            [
                api_param!("table", "table", "Table"),
                api_param!("index", "any", "Index")
            ],
            Some("any")
        ),
        api_function!(
            "rawset",
            "Raw table set.",
            [
                api_param!("table", "table", "Table"),
                api_param!("index", "any", "Index"),
                api_param!("value", "any", "Value")
            ],
            Some("table")
        ),
        api_function!(
            "rawequal",
            "Raw equality.",
            [
                api_param!("v1", "any", "Value 1"),
                api_param!("v2", "any", "Value 2")
            ],
            Some("boolean")
        ),
        api_function!(
            "require",
            "Load module.",
            [api_param!("modname", "string", "Module name")],
            Some("any")
        ),
        api_function!(
            "load",
            "Load chunk.",
            [
                api_param!("chunk", "string", "Chunk"),
                api_param!("chunkname", "string", "Chunk name")
            ],
            Some("function")
        ),
        api_function!(
            "loadfile",
            "Load file as chunk.",
            [api_param!("filename", "string", "File name")],
            Some("function")
        ),
        api_function!(
            "loadstring",
            "Load string as chunk.",
            [api_param!("string", "string", "String")],
            Some("function")
        ),
        api_function!(
            "dofile",
            "Execute file.",
            [api_param!("filename", "string", "File name")],
            Some("any")
        ),
        api_function!(
            "collectgarbage",
            "Garbage collector control.",
            [
                api_param!("opt", "string", "Option"),
                api_param!("arg", "any", "Argument")
            ],
            Some("any")
        ),
        api_function!(
            "getfenv",
            "Get function environment.",
            [api_param!("f", "any", "Function or level")],
            Some("table")
        ),
        api_function!(
            "setfenv",
            "Set function environment.",
            [
                api_param!("f", "any", "Function or level"),
                api_param!("table", "table", "Environment")
            ],
            Some("function")
        ),
        api_function!(
            "unpack",
            "Unpack table to values.",
            [
                api_param!("list", "table", "List"),
                api_param!("i", "number", "Start"),
                api_param!("j", "number", "End")
            ],
            Some("any")
        ),
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
            [api_param!(
                "config",
                "ServerConfig",
                "Server configuration table"
            )],
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
        api_function!(
            "rover_guard_string",
            "Create string guard.",
            [],
            Some("Guard<String>")
        ),
        api_function!(
            "rover_guard_integer",
            "Create integer guard.",
            [],
            Some("Guard<Integer>")
        ),
        api_function!(
            "rover_guard_number",
            "Create number guard.",
            [],
            Some("Guard<Number>")
        ),
        api_function!(
            "rover_guard_boolean",
            "Create boolean guard.",
            [],
            Some("Guard<Boolean>")
        ),
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
        api_function!(
            "rover_response_json",
            "Build JSON response. Can chain :status(code, data).",
            [],
            Some("RoverResponse")
        ),
        api_function!(
            "rover_response_text",
            "Build text response. Can chain :status(code, text).",
            [],
            Some("RoverResponse")
        ),
        api_function!(
            "rover_response_html",
            "Build HTML response. Can chain :status(code, html).",
            [],
            Some("RoverResponse")
        ),
        api_function!(
            "rover_response_error",
            "Build error response with status code and message.",
            [],
            Some("RoverResponse")
        ),
        api_function!(
            "rover_response_redirect",
            "Build redirect response. Can chain :permanent() or :status().",
            [],
            Some("RoverResponse")
        ),
        api_function!(
            "rover_response_no_content",
            "Build 204 No Content response.",
            [],
            Some("RoverResponse")
        ),
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
        api_object!(
            "string",
            "Lua string library.",
            [
                api_member!("byte" => "string_byte", "Returns internal numeric codes", method),
                api_member!("char" => "string_char", "Converts numeric codes to string", method),
                api_member!("dump" => "string_dump", "Returns string with binary representation", method),
                api_member!("find" => "string_find", "Find pattern in string", method),
                api_member!("format" => "string_format", "Format string", method),
                api_member!("gfind" => "string_gfind", "Global find iterator", method),
                api_member!("gsub" => "string_gsub", "Global substitution", method),
                api_member!("len" => "string_len", "String length", method),
                api_member!("lower" => "string_lower", "Lowercase string", method),
                api_member!("match" => "string_match", "Pattern match", method),
                api_member!("rep" => "string_rep", "Repeat string", method),
                api_member!("reverse" => "string_reverse", "Reverse string", method),
                api_member!("sub" => "string_sub", "Substring", method),
                api_member!("upper" => "string_upper", "Uppercase string", method)
            ]
        ),
        api_object!(
            "table",
            "Lua table library.",
            [
                api_member!("concat" => "table_concat", "Concatenate tables", method),
                api_member!("insert" => "table_insert", "Insert element", method),
                api_member!("maxn" => "table_maxn", "Maximum numeric index", method),
                api_member!("remove" => "table_remove", "Remove element", method),
                api_member!("sort" => "table_sort", "Sort table", method)
            ]
        ),
        api_object!(
            "math",
            "Lua math library.",
            [
                api_member!("abs" => "math_abs", "Absolute value", method),
                api_member!("acos" => "math_acos", "Arc cosine", method),
                api_member!("asin" => "math_asin", "Arc sine", method),
                api_member!("atan2" => "math_atan2", "Arc tangent (y, x)", method),
                api_member!("atan" => "math_atan", "Arc tangent", method),
                api_member!("ceil" => "math_ceil", "Ceiling", method),
                api_member!("cosh" => "math_cosh", "Hyperbolic cosine", method),
                api_member!("cos" => "math_cos", "Cosine", method),
                api_member!("deg" => "math_deg", "Radians to degrees", method),
                api_member!("exp" => "math_exp", "Exponential", method),
                api_member!("floor" => "math_floor", "Floor", method),
                api_member!("fmod" => "math_fmod", "Modulo", method),
                api_member!("frexp" => "math_frexp", "Split float", method),
                api_member!("huge" => "number", "Largest representable number", field),
                api_member!("ldexp" => "math_ldexp", "Combine exponent", method),
                api_member!("log10" => "math_log10", "Base-10 logarithm", method),
                api_member!("log" => "math_log", "Natural logarithm", method),
                api_member!("max" => "math_max", "Maximum", method),
                api_member!("min" => "math_min", "Minimum", method),
                api_member!("modf" => "math_modf", "Integer/fraction parts", method),
                api_member!("pi" => "number", "Pi constant", field),
                api_member!("pow" => "math_pow", "Power", method),
                api_member!("rad" => "math_rad", "Degrees to radians", method),
                api_member!("random" => "math_random", "Random number", method),
                api_member!("randomseed" => "math_randomseed", "Seed random generator", method),
                api_member!("sinh" => "math_sinh", "Hyperbolic sine", method),
                api_member!("sin" => "math_sin", "Sine", method),
                api_member!("sqrt" => "math_sqrt", "Square root", method),
                api_member!("tanh" => "math_tanh", "Hyperbolic tangent", method),
                api_member!("tan" => "math_tan", "Tangent", method)
            ]
        ),
        api_object!(
            "io",
            "Lua I/O library.",
            [
                api_member!("close" => "io_close", "Close file", method),
                api_member!("flush" => "io_flush", "Flush output", method),
                api_member!("input" => "io_input", "Read input", method),
                api_member!("lines" => "io_lines", "Read lines iterator", method),
                api_member!("open" => "io_open", "Open file", method),
                api_member!("output" => "io_output", "Write output", method),
                api_member!("popen" => "io_popen", "Open process", method),
                api_member!("read" => "io_read", "Read file", method),
                api_member!("type" => "io_type", "Check file type", method),
                api_member!("write" => "io_write", "Write file", method)
            ]
        ),
        api_object!(
            "os",
            "Lua OS library.",
            [
                api_member!("clock" => "os_clock", "Time", method),
                api_member!("date" => "os_date", "Date/time", method),
                api_member!("difftime" => "os_difftime", "Time difference", method),
                api_member!("execute" => "os_execute", "Execute command", method),
                api_member!("exit" => "os_exit", "Exit", method),
                api_member!("getenv" => "os_getenv", "Get environment variable", method),
                api_member!("remove" => "os_remove", "Remove file", method),
                api_member!("rename" => "os_rename", "Rename file", method),
                api_member!("setlocale" => "os_setlocale", "Set locale", method),
                api_member!("time" => "os_time", "Current time", method),
                api_member!("tmpname" => "os_tmpname", "Temporary filename", method)
            ]
        ),
        api_object!(
            "debug",
            "Lua debug library.",
            [
                api_member!("debug" => "debug_debug", "Enter debug mode", method),
                api_member!("getfenv" => "debug_getfenv", "Get environment", method),
                api_member!("gethook" => "debug_gethook", "Get hook", method),
                api_member!("getinfo" => "debug_getinfo", "Get debug info", method),
                api_member!("getlocal" => "debug_getlocal", "Get local variable", method),
                api_member!("getmetatable" => "debug_getmetatable", "Get metatable", method),
                api_member!("getregistry" => "debug_getregistry", "Get registry", method),
                api_member!("getupvalue" => "debug_getupvalue", "Get upvalue", method),
                api_member!("setfenv" => "debug_setfenv", "Set environment", method),
                api_member!("sethook" => "debug_sethook", "Set hook", method),
                api_member!("setlocal" => "debug_setlocal", "Set local variable", method),
                api_member!("setmetatable" => "debug_setmetatable", "Set metatable", method),
                api_member!("setupvalue" => "debug_setupvalue", "Set upvalue", method),
                api_member!("traceback" => "debug_traceback", "Get traceback", method)
            ]
        ),
        api_object!(
            "coroutine",
            "Lua coroutine library.",
            [
                api_member!("create" => "coroutine_create", "Create coroutine", method),
                api_member!("resume" => "coroutine_resume", "Resume coroutine", method),
                api_member!("running" => "coroutine_running", "Running coroutine", method),
                api_member!("status" => "coroutine_status", "Coroutine status", method),
                api_member!("wrap" => "coroutine_wrap", "Wrap function", method),
                api_member!("yield" => "coroutine_yield", "Yield execution", method)
            ]
        ),
        api_object!(
            "package",
            "Lua package library.",
            [
                api_member!("loaded" => "table", "Loaded packages", field),
                api_member!("loadlib" => "package_loadlib", "Load library", method),
                api_member!("seeall" => "table", "Seeall", field),
                api_member!("loaders" => "table", "Custom loaders", field),
                api_member!("preload" => "table", "Preload packages", field),
                api_member!("path" => "string", "Package search path", field),
                api_member!("cpath" => "string", "C library search path", field)
            ]
        ),
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
