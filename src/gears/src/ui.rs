use serde::{Serialize, Serializer};

pub type Id = String;

pub trait Ui {
    fn attach_main_view(&self, main_id: Id) -> ();
    fn create_view(&self, params: Params<ViewProps>) -> Id;
    fn create_text(&self, params: Params<TextProps>) -> Id;
}

#[derive(Debug)]
pub struct Params<T> {
    pub props: T,
    pub children: Vec<Id>,
}

impl<T> Params<T> {
    pub fn new(props: T) -> Self {
        Params {
            props,
            children: vec![],
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewProps {
    pub height: Option<Size>,
    pub width: Option<Size>,
    pub horizontal: Option<HorizontalAlignement>,
    pub vertical: Option<VerticalAlignement>,
    pub color: Option<String>,
}

impl ViewProps {
    pub fn new() -> Self {
        ViewProps {
            horizontal: None,
            vertical: None,
            color: None,
            height: None,
            width: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizontalAlignement {
    Left,
    Center,
    Right,
}

#[derive(Debug)]
pub enum Size {
    Full,
    Value(usize),
}

impl Serialize for Size {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            Size::Full => serializer.serialize_str("full"), // Use 100 or any other default value
            Size::Value(val) => serializer.serialize_u64(val as u64),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlignement {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextProps {
    pub color: Option<String>,
}

impl TextProps {
    pub fn new() -> Self {
        TextProps { color: None }
    }
}
