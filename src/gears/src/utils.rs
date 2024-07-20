use mlua::{Table, Value};

use crate::ui::{HorizontalAlignement, Params, Size, TextProps, VerticalAlignement, ViewProps};

pub fn parse_view_props_children(tbl: Table) -> Params<ViewProps> {
    let mut params = Params::new(ViewProps::new());

    for pair in tbl.pairs::<Value, Value>() {
        match pair.expect("Expected to have a pair") {
            (Value::String(prop), Value::String(value)) => match prop.as_bytes() {
                b"horizontal" => match value.as_bytes() {
                    b"center" => params.props.horizontal = Some(HorizontalAlignement::Center),
                    b"left" => params.props.horizontal = Some(HorizontalAlignement::Left),
                    b"right" => params.props.horizontal = Some(HorizontalAlignement::Right),
                    _ => panic!("Unexpected property value"),
                },
                b"vertical" => match value.as_bytes() {
                    b"center" => params.props.vertical = Some(VerticalAlignement::Center),
                    b"top" => params.props.vertical = Some(VerticalAlignement::Top),
                    b"bottom" => params.props.vertical = Some(VerticalAlignement::Bottom),
                    _ => panic!("Unexpected property value"),
                },
                b"color" => params.props.color = Some(value.to_str().unwrap().to_string()),
                b"height" => match value.as_bytes() {
                    b"full" => params.props.height = Some(Size::Full),
                    bytes => {
                        let number_str = std::str::from_utf8(bytes).unwrap();
                        if let Ok(number) = number_str.parse::<usize>() {
                            params.props.height = Some(Size::Value(number));
                        }
                    }
                },
                b"width" => match value.as_bytes() {
                    b"full" => params.props.width = Some(Size::Full),
                    bytes => {
                        let number_str = std::str::from_utf8(bytes).unwrap();
                        if let Ok(number) = number_str.parse::<usize>() {
                            params.props.width = Some(Size::Value(number));
                        }
                    }
                },
                _ => panic!("Unexpected property"),
            },
            (Value::Integer(_), Value::String(child_id)) => {
                params.children.push(child_id.to_str().unwrap().to_string())
            }
            _ => (),
        }
    }
    params
}

pub fn parse_text_props_children(tbl: Table) -> Params<TextProps> {
    let mut params = Params::new(TextProps::new());

    for pair in tbl.pairs::<Value, Value>() {
        match pair.expect("Expected to have a pair") {
            (Value::String(prop), Value::String(value)) => match prop.as_bytes() {
                b"color" => params.props.color = Some(value.to_str().unwrap().to_string()),
                _ => panic!("Unexpected property"),
            },
            (Value::Integer(_), Value::String(text)) => {
                params.children.push(text.to_str().unwrap().to_string())
            }
            _ => (),
        }
    }
    params
}
