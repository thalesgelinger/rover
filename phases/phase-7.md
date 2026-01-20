# Fase 7: Android Renderer

**Status:** 🔲 Não Iniciado  
**Duration:** 2-3 semanas  
**Dependencies:** Fase 5

## Objetivo

Segunda plataforma mobile. Mesma arquitetura, renderer diferente.

## Entregas

### 7.1 Rust → JNI Bridge

```rust
#[no_mangle]
pub extern "system" fn Java_com_rover_RoverBridge_createView(
    env: JNIEnv,
    _: JClass,
    node_type: jint,
) -> jobject {
    // Cria View via JNI
}

#[no_mangle]
pub extern "system" fn Java_com_rover_RoverBridge_updateText(
    env: JNIEnv,
    _: JClass,
    view: jobject,
    text: JString,
) {
    let text: String = env.get_string(text).unwrap().into();
    env.call_method(view, "setText", "(Ljava/lang/CharSequence;)V", &[JValue::Object(text.into())]).unwrap();
}
```

### 7.2 Android Renderer

```rust
pub struct AndroidRenderer {
    env: JNIEnv,
    node_refs: HashMap<NodeId, jobject>,
}

impl Renderer for AndroidRenderer {
    fn apply(&mut self, cmd: RenderCommand) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                let view = self.node_refs[&node];
                let jstr = self.env.new_string(&value).unwrap();
                self.env.call_method(view, "setText", "(Ljava/lang/CharSequence;)V", &[jstr.into()]).unwrap();
            }
        }
    }
}
```

## Validação Fase 7

- Mesmo código Lua roda no Android
- Paridade visual com iOS (tokens resolvem pra valores apropriados)
