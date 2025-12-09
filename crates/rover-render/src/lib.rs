use anyhow::{anyhow, Result};
use rover_lua::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Dimension {
    Auto,
    Full,
    Px(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewNode {
    pub kind: String,
    #[serde(default)]
    pub children: Vec<ViewNode>,
    pub text: Option<String>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub action: Option<String>,
}

impl ViewNode {
    pub fn from_value(value: &Value) -> Result<Self> {
        match value.clone() {
            Value::Table(table) => {
                let kind: String = table
                    .get("kind")
                    .map_err(|_| anyhow!("view table missing kind"))?;

                let width = parse_dimension(table.get::<_, Option<Value>>("width")?)?;
                let height = parse_dimension(table.get::<_, Option<Value>>("height")?)?;

                let mut children = Vec::new();
                for val in table.clone().sequence_values::<Value>() {
                    let child = val?;
                    if let Value::Table(_) = child {
                        children.push(ViewNode::from_value(&child)?);
                    }
                }

                let text = if kind == "text" || kind == "button" {
                    table.get::<_, Option<String>>(1).ok().flatten()
                } else {
                    None
                };

                let action = match table.get::<_, Option<Value>>("on_click")? {
                    Some(Value::String(s)) => Some(s.to_string_lossy().into_owned()),
                    Some(Value::Integer(i)) => Some(i.to_string()),
                    Some(Value::Number(n)) => Some(n.to_string()),
                    Some(Value::Boolean(b)) => Some(b.to_string()),
                    _ => None,
                };

                Ok(ViewNode {
                    kind,
                    children,
                    text,
                    width,
                    height,
                    action,
                })
            }
            _ => Err(anyhow!("expected render to return table")),
        }
    }
}

fn parse_dimension(value: Option<Value>) -> Result<Option<Dimension>> {
    match value {
        None | Some(Value::Nil) => Ok(None),
        Some(Value::String(s)) => {
            let txt = s.to_string_lossy();
            if txt == "full" {
                Ok(Some(Dimension::Full))
            } else {
                Err(anyhow!("unknown dimension string {txt}"))
            }
        }
        Some(Value::Integer(i)) => Ok(Some(Dimension::Px(i as f32))),
        Some(Value::Number(n)) => Ok(Some(Dimension::Px(n as f32))),
        _ => Err(anyhow!("invalid dimension value")),
    }
}
