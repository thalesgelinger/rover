use rover_ui::app::App;
use rover_ui::ui::StubRenderer;

#[test]
fn test_host_runtime_exposes_jit_table() {
    let app = App::new(StubRenderer::new()).unwrap();
    let has_jit: bool = app
        .lua()
        .load("return type(jit) == 'table'")
        .eval()
        .unwrap();
    assert!(has_jit);
}
