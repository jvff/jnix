use crate::{IntoJava, JnixEnv};
use jni::{
    objects::{AutoLocal, JObject},
    sys::{jboolean, jdouble, jint, jshort, jsize, JNI_FALSE, JNI_TRUE},
};

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for bool {
    const JNI_SIGNATURE: &'static str = "Z";

    type JavaType = jboolean;

    fn into_java(self, _: &'borrow JnixEnv<'env>) -> Self::JavaType {
        if self {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for i16 {
    const JNI_SIGNATURE: &'static str = "S";

    type JavaType = jshort;

    fn into_java(self, _: &'borrow JnixEnv<'env>) -> Self::JavaType {
        self as jshort
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for i32 {
    const JNI_SIGNATURE: &'static str = "I";

    type JavaType = jint;

    fn into_java(self, _: &'borrow JnixEnv<'env>) -> Self::JavaType {
        self as jint
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for f64 {
    const JNI_SIGNATURE: &'static str = "D";

    type JavaType = jdouble;

    fn into_java(self, _: &'borrow JnixEnv<'env>) -> Self::JavaType {
        self as jdouble
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for &'_ [u8] {
    const JNI_SIGNATURE: &'static str = "[B";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        let size = self.len();
        let array = env
            .new_byte_array(size as jsize)
            .expect("Failed to create a Java array of bytes");

        let data = unsafe { std::slice::from_raw_parts(self.as_ptr() as *const i8, size) };

        env.set_byte_array_region(array, 0, data)
            .expect("Failed to copy bytes to Java array");

        env.auto_local(JObject::from(array))
    }
}
