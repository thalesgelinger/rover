pub type Id = String;

pub trait Ui {
    fn create_view(& self, params: Params<ViewProps>) -> Id;
    fn create_text(& self, params: Params<TextProps>) -> Id;
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

#[derive(Debug)]
pub struct ViewProps {
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
        }
    }
}

#[derive(Debug)]
pub enum HorizontalAlignement {
    Left,
    Center,
    Right,
}

#[derive(Debug)]
pub enum VerticalAlignement {
    Top,
    Center,
    Bottom,
}

#[derive(Debug)]
pub struct TextProps {
    pub color: Option<String>,
}

impl TextProps {
    pub fn new() -> Self {
        TextProps { color: None }
    }
}
