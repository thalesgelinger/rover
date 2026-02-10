use rover_ui::app::App;
use rover_ui::ui::{StubRenderer, StyleOp};

#[test]
fn test_modifier_exists_and_is_extendable() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let (before_debug, after_debug): (String, String) = app
        .lua()
        .load(
            r#"
            local ui = rover.ui
            local before_type = type(ui.mod.debug)
            function ui.mod:debug()
                return self:border_color("danger"):border_width(1)
            end
            local after_type = type(ui.mod.debug)
            return before_type, after_type
        "#,
        )
        .eval()
        .unwrap();

    assert_eq!(before_debug, "nil");
    assert_eq!(after_debug, "function");
}

#[test]
fn test_reactive_modifier_updates_style() {
    let renderer = StubRenderer::new();
    let mut app = App::new(renderer).unwrap();

    app.lua()
        .load(
            r##"
            local ui = rover.ui
            local mod = ui.mod

            _G.bg = rover.signal("#111111")

            function rover.render()
                return ui.view {
                    mod = mod:bg_color(_G.bg),
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
    assert!(initial_style
        .ops
        .iter()
        .any(|op| matches!(op, StyleOp::BgColor(v) if v == "#111111")));

    app.lua().load("_G.bg.val = '#22aa22'").exec().unwrap();
    app.tick().unwrap();

    let updated_style = app
        .registry()
        .borrow()
        .get_node_style(root)
        .cloned()
        .unwrap();
    assert!(updated_style
        .ops
        .iter()
        .any(|op| matches!(op, StyleOp::BgColor(v) if v == "#22aa22")));
}

#[test]
fn test_theme_set_and_extend_affect_mod_resolution() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let (before, after_extend, after_set): (i64, i64, i64) = app
        .lua()
        .load(
            r##"
            local ui = rover.ui
            local mod = ui.mod

            local before = mod:padding("sm"):resolve().ops[1].value

            ui.extend_theme({ space = { sm = 9 } })
            local after_extend = mod:padding("sm"):resolve().ops[1].value

            ui.set_theme({
              space = { sm = 3 },
              color = { surface = "#123456" },
            })
            local after_set = mod:padding("sm"):resolve().ops[1].value

            return before, after_extend, after_set
        "##,
        )
        .eval()
        .unwrap();

    assert_eq!(before, 2);
    assert_eq!(after_extend, 9);
    assert_eq!(after_set, 3);
}
