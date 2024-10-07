use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use jni::objects::{JClass, JObject, JString, JValue};
use jni::JNIEnv;
use uuid::Uuid;

use crate::dev_server::{DevServer, ServerMessages};
use crate::lua::Rover;
use crate::ui::{Id, Params, TextProps, Ui, ViewProps};

#[no_mangle]
pub extern "system" fn Java_com_rovernative_roverandroid_Gears_start(
    env: JNIEnv<'static>,
    _: JClass,
    context: JObject<'static>,
    path: JString,
) {
    let env = Arc::new(Mutex::new(env));

    env.lock().unwrap().log_info("ROVER STARTED");

    let android = Arc::new(Android::new(context, Arc::clone(&env)));
    let rover = Rover::new(android);

    let path: String = env.lock().unwrap().get_string(&path).unwrap().into();

    match rover.start(path) {
        Ok(_) => env.lock().unwrap().log_info("Rover started"),
        Err(_) => env.lock().unwrap().log_error("Rover failed to start"),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_rovernative_roverandroid_Gears_devServer(
    mut env: JNIEnv<'static>,
    _: JClass,
    context: JObject<'static>,
    callback: JObject<'static>,
) {
    let (tx, rx) = mpsc::channel();

    let callback_global = env
        .new_global_ref(callback)
        .expect("Failed to create global ref");

    let dev_server = DevServer::new("10.0.2.2:4242");

    thread::spawn(move || {
        let _ = dev_server.listen(&tx);
    });

    let mut name = "".to_string();

    for received in rx {
        match received {
            ServerMessages::Project(project_name) => {
                name = project_name.clone();
                let file_class = env
                    .find_class("com/rovernative/roverandroid/FileUtils")
                    .unwrap();

                let file_utils = env.alloc_object(file_class).unwrap();

                let jstring = env.new_string(project_name).unwrap();

                env.call_method(
                    file_utils,
                    "createFolderIfNotExists",
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/io/File;",
                    &[JValue::Object(&context), JValue::Object(&jstring)],
                )
                .unwrap();
            }
            ServerMessages::File(file) => {
                let file_class = env
                    .find_class("com/rovernative/roverandroid/FileUtils")
                    .unwrap();

                let file_utils = env.alloc_object(file_class).unwrap();

                let jpath = env
                    .new_string(format!("{}/{}", name, file.path))
                    .unwrap();

                let jcontent = env.new_string(file.content).unwrap();

                env.call_method(
                    file_utils,
                    "writeFile",
                     "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&context), 
                        JValue::Object(&jpath), 
                        JValue::Object(&jcontent), 
                    ],
                )
                .unwrap();

                let jstring = env.new_string(format!("{}/lib/main.lua", name)).unwrap();
                match env.call_method(
                    callback_global.clone(),
                    "run",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&jstring)],
                ) {
                    Ok(_) => env.log_info("Updated from file changes"),
                    Err(_) => env.log_error("Fail to update from file changes"),
                }
            }
            ServerMessages::Ready => {
                let jstring = env.new_string(format!("{}/lib/main.lua", name)).unwrap();
                match env.call_method(
                    callback_global.clone(),
                    "run",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&jstring)],
                ) {
                    Ok(_) => env.log_info("Updated from file changes"),
                    Err(_) => env.log_error("Fail to update from file changes"),
                }
            }
        }
    }
}

struct Android {
    context: RefCell<JObject<'static>>,
    env: Arc<Mutex<JNIEnv<'static>>>,
    components: RefCell<HashMap<String, AndroidComponent>>,
    gears_android: Rc<JObject<'static>>,
}

impl Android {
    pub fn new(context: JObject<'static>, env: Arc<Mutex<JNIEnv<'static>>>) -> Android {
        let components = RefCell::new(HashMap::new());
        let gears_class = match env
            .lock()
            .unwrap()
            .find_class("com/rovernative/roverandroid/Gears")
        {
            Ok(class) => class,
            Err(e) => {
                env.lock()
                    .unwrap()
                    .throw_new(
                        "java/lang/RuntimeException",
                        format!("Failed to load the target class: {:?}", e),
                    )
                    .expect("Failed to throw exception");
                panic!("Failed to load Gears: {}", e)
            }
        };

        let gears_android = match env.lock().unwrap().alloc_object(gears_class) {
            Ok(value) => Rc::new(value),
            Err(e) => {
                env.lock()
                    .unwrap()
                    .throw_new(
                        "java/lang/RuntimeException",
                        "Failed to create an instance of the target class",
                    )
                    .expect("Failed to throw exception");
                panic!("Failed to load Gears: {}", e)
            }
        };
        env.lock().unwrap().log_info("ANDROID CREATED");

        Android {
            context: RefCell::new(context),
            env,
            components,
            gears_android,
        }
    }
}

#[derive(Debug)]
enum AndroidComponent {
    View(JObject<'static>),
    Text(JObject<'static>),
}

impl AndroidComponent {
    pub fn get_j_object(&self) -> Rc<&JObject<'static>> {
        match self {
            AndroidComponent::View(view) => Rc::new(view),
            AndroidComponent::Text(text) => Rc::new(text),
        }
    }
}

impl<'lua> Ui<'lua> for Android {
    fn attach_main_view(&self, main_id: Id) -> () {
        let components = self.components.borrow();
        let context = self.context.borrow();
        let child = components.get(&main_id).expect("Missing maing view");
        self.env.lock().unwrap().log_info("ATTACHING MAIN VIEW");

        match child {
            AndroidComponent::View(view) => {
                self.env
                    .lock()
                    .unwrap()
                    .log_info("ATTACHING VIEW ON MAIN VIEW");

                self.env
                    .lock()
                    .unwrap()
                    .call_method(
                        context.as_ref(),
                        "setContentView",
                        "(Landroid/view/View;)V",
                        &[view.into()],
                    )
                    .expect("Error in an attempt to call setContentView from rover");
            }
            AndroidComponent::Text(_) => self.env.lock().unwrap().log_error(
                "Attack text is not allowed in main view, please use a container object".into(),
            ),
        }
    }

    fn create_view(&self, params: Params<ViewProps>) -> Id {
        let id = format!("ROVER_VIEW_{}", Uuid::new_v4().to_string());
        let mut env = self.env.lock().unwrap();
        env.log_info("CREATING VIEW");

        let props = match env.new_string(params.props.to_json()) {
            Ok(value) => value,
            Err(_) => {
                env.log_error("Error creating text string:");
                panic!("Error creating text");
            }
        };

        env.log_info("Props Created");

        let result = env
            .call_method(
                self.gears_android.clone(),
                "createView",
                "(Landroid/app/Activity;Ljava/lang/String;)Landroid/view/View;",
                &[
                    JValue::Object(&self.context.borrow()),
                    JValue::Object(&props),
                ],
            )
            .expect("Failed creating view")
            .l()
            .expect("Failed to extract view object");

        for child_id in params.children {
            let components = self.components.borrow();
            let child = components
                .get(&child_id)
                .expect("Expected component to exist");

            env.log_info("ADDING CHILD");

            if let Err(_) = env.call_method(
                result.as_ref(),
                "addView",
                "(Landroid/view/View;)V",
                &[JValue::Object(child.get_j_object().as_ref())],
            ) {
                env.log_error("Failed to add view child")
            }
        }
        env.log_info("CHILDREN ADDED");

        self.components
            .borrow_mut()
            .insert(id.clone(), AndroidComponent::View(result));

        env.log_info("VIEW CREATED");

        id
    }

    fn create_text(&self, params: Params<TextProps>) -> Id {
        let id = format!("ROVER_TEXT_{}", Uuid::new_v4().to_string());
        let mut env = self.env.lock().unwrap();
        env.log_info("CREATING TEXT");

        let text = &params.children.join("\n");

        let jstring = match env.new_string(text) {
            Ok(value) => value,
            Err(_) => {
                env.log_error("Error creating text string:");
                panic!("");
            }
        };

        let result = env
            .call_method(
                self.gears_android.clone(),
                "createTextView",
                "(Landroid/app/Activity;Ljava/lang/String;)Landroid/widget/TextView;",
                &[
                    JValue::Object(&self.context.borrow()),
                    JValue::Object(&jstring),
                ],
            )
            .expect("Failed creating view")
            .l()
            .expect("Failed to extract view object");

        self.components
            .borrow_mut()
            .insert(id.clone(), AndroidComponent::Text(result));

        env.log_info("TEXT CREATED");
        id
    }

    fn create_button(&self, _params: Params<crate::ui::ButtonProps<'lua>>) -> Id {
        todo!()
    }
}

impl Log for JNIEnv<'static> {
    fn log_info(&mut self, msg: &str) {
        let log_class = self
            .find_class("android/util/Log")
            .expect("Failed to find Log class");
        let tag = self
            .new_string("ROVER")
            .expect("Failed to create Java string for tag");
        let msg = self
            .new_string(msg)
            .expect("Failed to create Java string for message");

        self.call_static_method(
            log_class,
            "i",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&tag.into()), JValue::Object(&msg.into())],
        )
        .expect("Failed to call Log.i method");
    }

    fn log_error(&mut self, msg: &str) {
        let log_class = self
            .find_class("android/util/Log")
            .expect("Failed to find Log class");
        let tag = self
            .new_string("ROVER")
            .expect("Failed to create Java string for tag");
        let msg = self
            .new_string(msg)
            .expect("Failed to create Java string for message");

        self.call_static_method(
            log_class,
            "e",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&tag.into()), JValue::Object(&msg.into())],
        )
        .expect("Failed to call Log.i method");
    }
}

trait Log {
    fn log_info(&mut self, msg: &str) -> ();

    fn log_error(&mut self, msg: &str) -> ();
}
