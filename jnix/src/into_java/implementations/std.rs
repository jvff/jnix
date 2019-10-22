use crate::IntoJava;
use jni::{
    objects::{AutoLocal, JObject},
    sys::{jboolean, jdouble, JNI_FALSE, JNI_TRUE},
    JNIEnv,
};

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for bool {
    const JNI_SIGNATURE: &'static str = "Z";

    type JavaType = jboolean;

    fn into_java(self, _: &'borrow JNIEnv<'env>) -> Self::JavaType {
        if self {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for f64 {
    const JNI_SIGNATURE: &'static str = "D";

    type JavaType = jdouble;

    fn into_java(self, _: &'borrow JNIEnv<'env>) -> Self::JavaType {
        self as jdouble
    }
}

impl<'borrow, 'env, T> IntoJava<'borrow, 'env> for Option<T>
where
    'env: 'borrow,
    T: IntoJava<'borrow, 'env, JavaType = AutoLocal<'env, 'borrow>>,
{
    const JNI_SIGNATURE: &'static str = T::JNI_SIGNATURE;

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        match self {
            Some(t) => t.into_java(env),
            None => env.auto_local(JObject::null()),
        }
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for String {
    const JNI_SIGNATURE: &'static str = "Ljava/lang/String;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        let jstring = env.new_string(&self).expect("Failed to create Java String");

        env.auto_local(jstring.into())
    }
}
