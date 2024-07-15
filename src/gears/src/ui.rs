use std::rc::Rc;

pub trait Ui {
    fn create_view(&self) -> Box<dyn Component>;
    fn create_text(&self) -> Box<dyn Component>;
    fn get_component(&self, id: String) -> &Rc<dyn Component>;
    fn set_component(&mut self, id: String, component: Rc<dyn Component>) -> ();
}

pub trait Component {
    fn get_id(&self) -> String;
}
