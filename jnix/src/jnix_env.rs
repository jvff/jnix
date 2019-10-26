use jni::{
    objects::{GlobalRef, JObject},
    JNIEnv,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::{borrow::Cow, collections::HashMap, ops::Deref};

static CLASS_CACHE: Lazy<Mutex<HashMap<String, GlobalRef>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub struct JnixEnv<'env> {
    env: JNIEnv<'env>,
}

impl<'env> From<JNIEnv<'env>> for JnixEnv<'env> {
    fn from(env: JNIEnv<'env>) -> Self {
        JnixEnv { env }
    }
}

impl<'env> Deref for JnixEnv<'env> {
    type Target = JNIEnv<'env>;

    fn deref(&self) -> &Self::Target {
        &self.env
    }
}

impl<'env> JnixEnv<'env> {
    pub fn get_class<'a>(&self, class_name: impl Into<Cow<'a, str>>) -> GlobalRef {
        let class_name = class_name.into();
        let mut cache = CLASS_CACHE.lock();
        log::debug!("JnixEnv::get_class({})", class_name);

        if let Some(class) = cache.get(class_name.as_ref()) {
            class.clone()
        } else {
            let class = self.load_class(class_name.as_ref());

            cache.insert(class_name.into_owned(), class.clone());

            class
        }
    }

    pub fn preload_classes(&self, class_names: impl IntoIterator<Item = impl Into<String>>) {
        let mut cache = CLASS_CACHE.lock();

        for class_name in class_names {
            let class_name = class_name.into();
            let class = self.load_class(&class_name);

            cache.insert(class_name, class);
        }
    }

    fn load_class(&self, class_name: impl AsRef<str>) -> GlobalRef {
        let class_name = class_name.as_ref();
        log::debug!("JnixEnv::load_class({})", class_name);
        let local_ref = self
            .env
            .find_class(class_name)
            .expect(&format!("Failed to find {} Java class", class_name));

        self.env.new_global_ref(JObject::from(local_ref)).expect(
            "Failed to convert local reference to Java class object into a global reference",
        )
    }
}
