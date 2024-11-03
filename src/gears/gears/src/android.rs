use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use anyhow::{Error, Result};
use jni::objects::{JClass, JObject, JString, JValue};
use jni::JNIEnv;
use uuid::Uuid;

use crate::dev_server::{DevServer, ServerMessages};
use crate::lua::Rover;
use crate::ui::{ButtonProps, Id, Params, TextProps, Ui,  ViewProps};

#[no_mangle]
pub extern "system" fn Java_com_rovernative_roverandroid_Gears_start(
    env: JNIEnv<'static>,
    _: JClass,
    context: JObject<'static>,
    path: JString,
) {
    let env = Arc::new(Mutex::new(env));

    let _ = env.lock().unwrap().log_info("ROVER STARTED");

    let android = Arc::new(Android::new(context, Arc::clone(&env)));
    let rover = Rover::new(android);

    let path: String = env.lock().unwrap().get_string(&path).unwrap().into();

    match rover.start(path) {
        Ok(()) => env.lock().unwrap().log_info("Rover started").unwrap(),
        Err(err) => env.lock().unwrap().log_error(&err.to_string()).unwrap(),
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

                let _ = match env.call_method(
                    file_utils,
                    "createFolderIfNotExists",
                    "(Landroid/content/Context;Ljava/lang/String;)Ljava/io/File;",
                    &[JValue::Object(&context), JValue::Object(&jstring)],
                ) {
                    Ok(_) => env.log_info("Project created").unwrap(),
                    Err(_) => env.log_info("Error creating project").unwrap(),
                };
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
                let _ = match env.call_method(
                    callback_global.clone(),
                    "run",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&jstring)],
                ){
                    Ok(_) => env.log_info("Filed sent").unwrap(),
                    Err(_) => env.log_info("Error sending filed").unwrap(),
                };
            }
            ServerMessages::Ready => {
                let jstring = env.new_string(format!("{}/lib/main.lua", name)).unwrap();
                let _ = match                env.call_method(
                    callback_global.clone(),
                    "run",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&jstring)],
                ){
                    Ok(_) => env.log_info("Filed sent").unwrap(),
                    Err(_) => env.log_info("Error sending filed").unwrap(),
                };
            }
        };
    };
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
        let _ = env.lock().unwrap().log_info("ANDROID CREATED");

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

impl Ui for Android {
    fn attach_main_view(&self, main_id: Id) -> Result<()> {
        let components = self.components.borrow();
        let context = self.context.borrow();
        let child = components.get(&main_id).ok_or_else(|| Error::msg("There's no chiled to be attached to main view"))?;
        self.env.lock().unwrap().log_info("ATTACHING MAIN VIEW")?;

        match child {
            AndroidComponent::View(view) => {
                self.env
                    .lock()
                    .unwrap()
                    .log_info("ATTACHING VIEW ON MAIN VIEW")?;

                self.env
                    .lock()
                    .unwrap()
                    .call_method(
                        context.as_ref(),
                        "setContentView",
                        "(Landroid/view/View;)V",
                        &[view.into()],
                    )?;
            }
            AndroidComponent::Text(_) => self.env.lock().unwrap().log_error(
                "Attack text is not allowed in main view, please use a container object".into(),
            )?
        };
        Ok(())
    }

    fn create_view(&self, params: Params<ViewProps>) -> Result<Id> {
        let id = format!("ROVER_VIEW_{}", Uuid::new_v4().to_string());
        let mut env = self.env.lock().unwrap();
        env.log_info("CREATING VIEW")?;

        let props = env.new_string(params.props.to_json())?;

        env.log_info("Props Created")?;

        let result = env
            .call_method(
                self.gears_android.clone(),
                "createView",
                "(Landroid/app/Activity;Ljava/lang/String;)Landroid/view/View;",
                &[
                    JValue::Object(&self.context.borrow()),
                    JValue::Object(&props),
                ],
            )?.l()?;

        for child_id in params.children {
            let components = self.components.borrow();
            let child = components
                .get(&child_id)
                .ok_or_else(|| Error::msg("Expected component to exist"))?;

            env.log_info("ADDING CHILD")?;

            env.call_method(
                result.as_ref(),
                "addView",
                "(Landroid/view/View;)V",
                &[JValue::Object(child.get_j_object().as_ref())],
            )?;
        }
        env.log_info("CHILDREN ADDED")?;

        self.components
            .borrow_mut()
            .insert(id.clone(), AndroidComponent::View(result));

        env.log_info("VIEW CREATED")?;

        Ok(id)
    }

    fn create_text(&self, params: Params<TextProps>) -> Result<Id> {
        let id = format!("ROVER_TEXT_{}", Uuid::new_v4().to_string());
        let mut env = self.env.lock().unwrap();
        env.log_info("CREATING TEXT")?;

        let text = &params.children.join("\n");

        let jstring = env.new_string(text)?;

        let result = env
            .call_method(
                self.gears_android.clone(),
                "createTextView",
                "(Landroid/app/Activity;Ljava/lang/String;)Landroid/widget/TextView;",
                &[
                    JValue::Object(&self.context.borrow()),
                    JValue::Object(&jstring),
                ],
            )? .l()?;

        self.components
            .borrow_mut()
            .insert(id.clone(), AndroidComponent::Text(result));

        env.log_info("TEXT CREATED")?;
        Ok(id)
    }

    fn create_button(&self, _params: Params<ButtonProps>) -> Result<Id>{
        let id = format!("ROVER_BUTTON_{}", Uuid::new_v4().to_string());
        Ok(id)
    }
}

impl Log for JNIEnv<'static> {
    fn log_info(&mut self, msg: &str) -> Result<()> {
        let log_class = self .find_class("android/util/Log")?;
        let tag = self .new_string("ROVER")?;
        let msg = self .new_string(msg)?;

        self.call_static_method(
            log_class,
            "i",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&tag.into()), JValue::Object(&msg.into())],
        )?;
        Ok(())
    }

    fn log_error(&mut self, msg: &str) -> Result<()> {
        let log_class = self .find_class("android/util/Log")?;
        let tag = self .new_string("ROVER")?;
        let msg = self .new_string(msg)?;

        self.call_static_method(
            log_class,
            "e",
            "(Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&tag.into()), JValue::Object(&msg.into())],
        )?;
        Ok(())
    }
}

trait Log {
    fn log_info(&mut self, msg: &str) -> Result<()>;

    fn log_error(&mut self, msg: &str) -> Result<()>;
}
