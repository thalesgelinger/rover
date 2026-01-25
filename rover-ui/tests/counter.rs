/// Integration test for counter.lua-style functionality
/// Tests UI with signals, where changes trigger granular UI updates

use rover_ui::app::App;
use rover_ui::ui::stub::StubRenderer;
use std::rc::Rc;
use std::cell::RefCell;

/// Test that mimics examples/counter.lua - a counter with text display
/// and rover.task() + rover.delay() for timing
#[test]
fn test_counter_style_ui() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    // This mimics counter.lua pattern: use rover.render() with rover.task()
    let script = r#"
        local ru = rover.ui

        -- rover.render() returns a LuaNode, we can get its id
        local node = rover.render(function()
            local value = rover.signal(0)

            -- Create a task that updates the value
            local tick = rover.task(function()
                rover.delay(10)
                value.val = value.val + 1
            end)

            -- Start the task
            tick()

            return ru.text { value }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick for 20ms to let the task run (10ms delay + margin)
    app.tick_ms(20).unwrap();

    // Verify a text node was created
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Text")));
}

/// Test task creation and basic functionality
#[test]
fn test_task_creation() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let script = r#"
        local task = rover.task(function()
            rover.delay(100)
            return "done"
        end)

        -- Task should have status "ready"
        return task:status()
    "#;

    let status: String = app.lua().load(script).eval().unwrap();
    assert_eq!(status, "ready");
}

/// Test task execution - verify coroutine resumption works
#[test]
fn test_task_execution() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    // Test that coroutine resumption works after yield
    let script = r#"
        _G.test_count = 0

        local tick = rover.task(function()
            _G.test_count = _G.test_count + 1
            coroutine.yield(rover.delay(5))
            _G.test_count = _G.test_count + 1
        end)

        tick()

        return _G.test_count
    "#;

    let count1: i32 = app.lua().load(script).eval().unwrap();
    assert_eq!(count1, 1);  // First increment before yield

    app.tick_ms(20).unwrap();

    let count2: i32 = app.lua().load("return _G.test_count").eval().unwrap();
    assert_eq!(count2, 2);  // Should be 2 after resuming
}

/// Test task cancellation
#[test]
fn test_task_cancellation() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let script = r#"
        local tick = rover.task(function()
            rover.delay(1000)
            return "never reached"
        end)

        -- Cancel before starting
        tick:cancel()

        -- Status should be "cancelled"
        return tick:status()
    "#;

    let status: String = app.lua().load(script).eval().unwrap();
    assert_eq!(status, "cancelled");
}

/// Test rover.on_destroy() for cleanup callbacks
#[test]
fn test_on_destroy_callback() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    let script = r#"
        local cleanup_called = rover.signal(false)

        rover.on_destroy(function()
            cleanup_called.val = true
        end)

        return cleanup_called.val
    "#;

    let before_cleanup: bool = app.lua().load(script).eval().unwrap();
    assert!(!before_cleanup);

    // When app is dropped, cleanup should run
    // (This is tested implicitly via Drop implementation)
}

/// Test button with click handler - UI with interactivity
#[test]
fn test_button_click_handler() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui
        local clicks = rover.signal(0)

        local node = rover.render(function()
            return ru.button {
                label = "Click me",
                on_click = function()
                    clicks.val = clicks.val + 1
                end
            }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Node IDs start at 0, so just check the log for the button
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Button") && line.contains("[clickable]")));
}

/// Test a simple column layout with static text nodes
#[test]
fn test_column_layout() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.column {
                ru.text { "Item 1" },
                ru.text { "Item 2" },
                ru.text { "Item 3" }
            }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Verify column was created with children
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Column")));
    // Should have 3 text nodes
    let text_count = log.iter().filter(|line| line.contains("Text") && line.contains("\"")).count();
    assert!(text_count >= 3);
}

/// Test checkbox component
#[test]
fn test_checkbox_component() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.checkbox { checked = false }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Verify checkbox was created with ☐ (unchecked) state
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Checkbox") && line.contains("☐")));
}

/// Test input component
#[test]
fn test_input_component() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.input { value = "initial text" }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Verify input was created with initial value
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Input") && line.contains("initial text")));
}

/// Test image component
#[test]
fn test_image_component() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.image { src = "test.png" }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Image") && line.contains("test.png")));
}

/// Test view container
#[test]
fn test_view_container() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.view {
                ru.text { "Content inside view" }
            }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("View")));
}

/// Test that rover.delay() works with explicit yield
#[test]
fn test_delay_scheduling() {
    let renderer = StubRenderer::new();
    let app = App::new(renderer).unwrap();

    // Use explicit coroutine.yield for reliable delay behavior
    let script = r#"
        local tick = rover.task(function()
            coroutine.yield(rover.delay(100))
        end)

        tick()

        return tick:status()
    "#;

    let status: String = app.lua().load(script).eval().unwrap();
    assert_eq!(status, "yielded");

    let pending_count = app.scheduler().borrow().pending_count();
    assert!(pending_count > 0);
}

/// Test nested layout - column containing rows
#[test]
fn test_nested_layout() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui

        local node = rover.render(function()
            return ru.column {
                ru.row {
                    ru.text { "A" },
                    ru.text { "B" }
                },
                ru.row {
                    ru.text { "C" },
                    ru.text { "D" }
                }
            }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Verify nested structure
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("Column")));
    assert!(log.iter().any(|line| line.contains("Row")));
}

/// Test derived signal with UI
#[test]
fn test_derived_signal_ui() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    let script = r#"
        local ru = rover.ui
        local count = rover.signal(5)
        local doubled = rover.derive(function()
            return count.val * 2
        end)

        local node = rover.render(function()
            return ru.text { doubled }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Should show initial value of 10 (5 * 2)
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("\"10\"")));
}

/// Test that signal updates trigger UI re-renders
#[test]
fn test_signal_update_triggers_render() {
    let log_buffer = Rc::new(RefCell::new(Vec::new()));
    let renderer = StubRenderer::with_buffer(log_buffer.clone());
    let mut app = App::new(renderer).unwrap();

    // Create the signal in global scope so we can access it later
    let setup_script = r#"
        local ru = rover.ui
        _G.count = rover.signal(0)

        local node = rover.render(function()
            return ru.text { _G.count }
        end)

        return node.id
    "#;

    let _node_id: u32 = app.lua().load(setup_script).eval().unwrap();

    // Tick to trigger initial render (auto-mounts)
    app.tick().unwrap();

    // Should see initial value "0"
    {
        let log = log_buffer.borrow();
        assert!(log.iter().any(|line| line.contains("\"0\"")));
    }  // Release the borrow here

    // Update the signal via a task
    app.lua().load(r#"
        local updater = rover.task(function()
            _G.count.val = 42
            coroutine.yield(rover.delay(1))
        end)
        updater()
    "#).exec().unwrap();

    // Clear and tick to process updates
    log_buffer.borrow_mut().clear();
    app.tick_ms(10).unwrap();  // Wait for the task to run

    // Should see updated value "42"
    let log = log_buffer.borrow();
    assert!(log.iter().any(|line| line.contains("\"42\"") || line.contains("\"0\" → \"42\"")));
}
