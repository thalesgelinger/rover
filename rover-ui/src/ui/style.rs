use mlua::{Table, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleOp {
    Padding(u16),
    BgColor(String),
    BorderColor(String),
    BorderWidth(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleSize {
    Full,
    Px(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionType {
    Relative,
    Absolute,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeStyle {
    pub ops: Vec<StyleOp>,
    pub width: Option<StyleSize>,
    pub height: Option<StyleSize>,
    pub position: PositionType,
    pub top: Option<i32>,
    pub left: Option<i32>,
    pub right: Option<i32>,
    pub bottom: Option<i32>,
    pub grow: Option<f64>,
    pub gap: Option<u16>,
    pub justify: Option<String>,
    pub align: Option<String>,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            ops: Vec::new(),
            width: None,
            height: None,
            position: PositionType::Relative,
            top: None,
            left: None,
            right: None,
            bottom: None,
            grow: None,
            gap: None,
            justify: None,
            align: None,
        }
    }
}

impl NodeStyle {
    pub fn from_lua_table(table: &Table) -> mlua::Result<Self> {
        let mut style = Self::default();

        if let Ok(ops_table) = table.get::<Table>("ops") {
            for entry in ops_table.sequence_values::<Table>() {
                let op = entry?;
                let kind: String = op.get("kind")?;
                match kind.as_str() {
                    "padding" => {
                        if let Some(v) = to_u16(op.get::<Value>("value")?) {
                            style.ops.push(StyleOp::Padding(v));
                        }
                    }
                    "bg_color" => {
                        if let Some(v) = to_string(op.get::<Value>("value")?) {
                            style.ops.push(StyleOp::BgColor(v));
                        }
                    }
                    "border_color" => {
                        if let Some(v) = to_string(op.get::<Value>("value")?) {
                            style.ops.push(StyleOp::BorderColor(v));
                        }
                    }
                    "border_width" => {
                        if let Some(v) = to_u16(op.get::<Value>("value")?) {
                            style.ops.push(StyleOp::BorderWidth(v));
                        }
                    }
                    _ => {}
                }
            }
        }

        style.width = parse_size_opt(table.get::<Value>("width")?);
        style.height = parse_size_opt(table.get::<Value>("height")?);

        if let Some(pos) = to_string(table.get::<Value>("position")?) {
            style.position = if pos == "absolute" {
                PositionType::Absolute
            } else {
                PositionType::Relative
            };
        }

        style.top = to_i32(table.get::<Value>("top")?);
        style.left = to_i32(table.get::<Value>("left")?);
        style.right = to_i32(table.get::<Value>("right")?);
        style.bottom = to_i32(table.get::<Value>("bottom")?);

        style.grow = to_f64(table.get::<Value>("grow")?);
        style.gap = to_u16(table.get::<Value>("gap")?);
        style.justify = to_string(table.get::<Value>("justify")?);
        style.align = to_string(table.get::<Value>("align")?);

        Ok(style)
    }
}

fn parse_size_opt(value: Value) -> Option<StyleSize> {
    match value {
        Value::Nil => None,
        Value::String(s) => {
            if s.to_str().ok()? == "full" {
                Some(StyleSize::Full)
            } else {
                None
            }
        }
        Value::Integer(n) => Some(StyleSize::Px(n.max(0) as u16)),
        Value::Number(n) => Some(StyleSize::Px(n.max(0.0) as u16)),
        _ => None,
    }
}

fn to_u16(value: Value) -> Option<u16> {
    match value {
        Value::Integer(n) => Some(n.max(0) as u16),
        Value::Number(n) => Some(n.max(0.0) as u16),
        _ => None,
    }
}

fn to_i32(value: Value) -> Option<i32> {
    match value {
        Value::Integer(n) => Some(n as i32),
        Value::Number(n) => Some(n as i32),
        _ => None,
    }
}

fn to_f64(value: Value) -> Option<f64> {
    match value {
        Value::Integer(n) => Some(n as f64),
        Value::Number(n) => Some(n),
        _ => None,
    }
}

fn to_string(value: Value) -> Option<String> {
    match value {
        Value::String(s) => s.to_str().ok().map(|v| v.to_string()),
        _ => None,
    }
}
