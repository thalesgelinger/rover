pub type Id = String;

pub trait Ui {
    fn create_view(&self) -> Id;
    fn create_text(&self) -> Id;
}
