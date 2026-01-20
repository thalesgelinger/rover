# Fase 6: iOS Renderer (UIKit)

**Status:** 🔲 Não Iniciado  
**Duration:** 3-4 semanas  
**Dependencies:** Fase 5

## Objetivo

Primeira plataforma mobile nativa. Validar que arquitetura escala pra mobile.

## Entregas

### 6.1 Rust → Swift/ObjC Bridge

```rust
// Via swift-bridge ou manual FFI
#[repr(C)]
pub struct RoverBridge {
    // Callbacks pro Swift
    create_view: extern "C" fn(NodeType) -> *mut c_void,
    update_text: extern "C" fn(*mut c_void, *const c_char),
    insert_subview: extern "C" fn(*mut c_void, *mut c_void, usize),
    // ...
}
```

```swift
// Swift side
class RoverRenderer {
    var viewRefs: [UInt32: UIView] = [:]
    
    func createView(_ nodeType: NodeType) -> UnsafeMutableRawPointer {
        let view: UIView
        switch nodeType {
        case .text:
            view = UILabel()
        case .column:
            view = UIStackView()
            (view as! UIStackView).axis = .vertical
        case .row:
            view = UIStackView()
            (view as! UIStackView).axis = .horizontal
        case .button:
            view = UIButton(type: .system)
        }
        // Store and return pointer
    }
    
    func updateText(_ viewPtr: UnsafeMutableRawPointer, _ text: UnsafePointer<CChar>) {
        let label = Unmanaged<UILabel>.fromOpaque(viewPtr).takeUnretainedValue()
        label.text = String(cString: text)
    }
}
```

### 6.2 iOS Renderer Implementation

```rust
pub struct IosRenderer {
    bridge: RoverBridge,
    node_refs: HashMap<NodeId, *mut c_void>,
}

impl Renderer for IosRenderer {
    fn create_node(&mut self, node_type: NodeType) -> IosHandle {
        let ptr = (self.bridge.create_view)(node_type);
        IosHandle(ptr)
    }
    
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let ptr = self.node_refs[&node];
                let c_str = CString::new(value).unwrap();
                (self.bridge.update_text)(ptr, c_str.as_ptr());
            }
            RenderCommand::InsertChild { parent, index, child } => {
                let parent_ptr = self.node_refs[&parent];
                let child_ptr = self.node_refs[&child];
                (self.bridge.insert_subview)(parent_ptr, child_ptr, index);
            }
        }
    }
}
```

### 6.3 Layout com Auto Layout ou Manual

*Opção A: Auto Layout*

```swift
func applyLayout(_ viewPtr: UnsafeMutableRawPointer, _ constraints: LayoutConstraints) {
    let view = Unmanaged<UIView>.fromOpaque(viewPtr).takeUnretainedValue()
    view.translatesAutoresizingMaskIntoConstraints = false
    
    NSLayoutConstraint.activate([
        view.widthAnchor.constraint(equalToConstant: constraints.width),
        // ...
    ])
}
```

*Opção B: Manual frames (mais controle)*

```swift
func applyLayout(_ viewPtr: UnsafeMutableRawPointer, _ frame: CGRect) {
    let view = Unmanaged<UIView>.fromOpaque(viewPtr).takeUnretainedValue()
    view.frame = frame
}
```

### 6.4 Token Resolver iOS

```rust
pub struct IosTokenResolver {
    scale: f32,  // UIScreen.main.scale
}

impl TokenResolver for IosTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::Xs => 4.0 * self.scale,
            SpacingToken::Sm => 8.0 * self.scale,
            SpacingToken::Md => 16.0 * self.scale,
            SpacingToken::Lg => 24.0 * self.scale,
            SpacingToken::Xl => 32.0 * self.scale,
        }
    }
}
```

## Validação Fase 6

- Mesmo código Lua do LoginForm roda no iOS
- Performance: 60fps em scroll de lista
- Memory: baseline < 20MB
