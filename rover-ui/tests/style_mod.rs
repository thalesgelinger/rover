use rover_ui::app::App;
use rover_ui::ui::{StubRenderer, StyleOp};

#[test]
fn test_style_object_is_supported() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let background: String = app
        .lua()
        .load(
            r#"
            local ui = rover.ui
            local node = ui.view {
                style = { bg_color = "surface" },
                ui.text { "x" }
            }
            rover.render = function() return node end
            return node ~= nil and "ok" or "no"
        "#,
        )
        .eval()
        .unwrap();

    assert_eq!(background, "ok");
}

#[test]
fn test_reactive_style_updates_style() {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).unwrap();

    app.lua()
        .load(
            r##"
            local ui = rover.ui
            _G.bg = rover.signal("#111111")

            function rover.render()
                return ui.view {
                    style = { bg_color = _G.bg },
                    ui.text { "x" },
                }
            end
        "##,
        )
        .exec()
        .unwrap();

    app.tick().unwrap();

    let root = app.registry().borrow().root().unwrap();
    let initial_style = app
        .registry()
        .borrow()
        .get_node_style(root)
        .cloned()
        .unwrap();
    assert!(
        initial_style
            .ops
            .iter()
            .any(|op| matches!(op, StyleOp::BgColor(v) if v == "#111111"))
    );

    app.lua().load("_G.bg.val = '#22aa22'").exec().unwrap();
    app.tick().unwrap();

    let updated_style = app
        .registry()
        .borrow()
        .get_node_style(root)
        .cloned()
        .unwrap();
    assert!(
        updated_style
            .ops
            .iter()
            .any(|op| matches!(op, StyleOp::BgColor(v) if v == "#22aa22"))
    );
}

#[test]
fn test_theme_set_and_extend_affect_style_resolution() {
    fn padding_after(setup: &str) -> u16 {
        let renderer = StubRenderer::new();
        let mut app = App::new(renderer).unwrap();
        app.lua().load(setup).exec().unwrap();
        app.lua()
            .load(
                r##"
                local ui = rover.ui
                function rover.render()
                    return ui.view { style = { padding = "sm" }, ui.text { "x" } }
                end
            "##,
            )
            .exec()
            .unwrap();
        app.tick().unwrap();
        let root = app.registry().borrow().root().unwrap();
        let style = app
            .registry()
            .borrow()
            .get_node_style(root)
            .cloned()
            .unwrap();
        style
            .ops
            .iter()
            .find_map(|op| match op {
                StyleOp::Padding(v) => Some(*v),
                _ => None,
            })
            .unwrap_or(0)
    }

    assert_eq!(padding_after(""), 2);
    assert_eq!(
        padding_after("rover.ui.extend_theme({ space = { sm = 9 } })"),
        9
    );
    assert_eq!(
        padding_after(
            "rover.ui.set_theme({ space = { sm = 3 }, color = { surface = '#123456' } })"
        ),
        3
    );
}

#[test]
fn test_color_style_resolves_theme_token() {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).unwrap();

    app.lua()
        .load(
            r##"
            local ui = rover.ui
            function rover.render()
                return ui.text { "x", style = { color = "accent" } }
            end
        "##,
        )
        .exec()
        .unwrap();

    app.tick().unwrap();
    let root = app.registry().borrow().root().unwrap();
    let style = app
        .registry()
        .borrow()
        .get_node_style(root)
        .cloned()
        .unwrap();
    assert_eq!(style.color.as_deref(), Some("#22c55e"));
}

#[test]
fn test_reactive_scalar_style_updates_style() {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).unwrap();

    app.lua()
        .load(
            r##"
            local ui = rover.ui
            _G.pos_x = rover.signal(2)
            _G.pos_y = rover.signal(3)

            function rover.render()
                return ui.stack {
                    ui.view {
                        style = { position = "absolute", left = _G.pos_x, top = _G.pos_y },
                        ui.text { "x" },
                    },
                }
            end
        "##,
        )
        .exec()
        .unwrap();

    app.tick().unwrap();

    let root = app.registry().borrow().root().unwrap();
    let child = {
        let reg = app.registry().borrow();
        match reg.get_node(root).unwrap() {
            rover_ui::ui::UiNode::Stack { children } => children[0],
            _ => panic!("expected stack root"),
        }
    };

    let initial = app
        .registry()
        .borrow()
        .get_node_style(child)
        .cloned()
        .unwrap();
    assert_eq!(initial.left, Some(2));
    assert_eq!(initial.top, Some(3));

    app.lua()
        .load("_G.pos_x.val = 9; _G.pos_y.val = 11")
        .exec()
        .unwrap();
    app.tick().unwrap();

    let updated = app
        .registry()
        .borrow()
        .get_node_style(child)
        .cloned()
        .unwrap();
    assert_eq!(updated.left, Some(9));
    assert_eq!(updated.top, Some(11));
}
