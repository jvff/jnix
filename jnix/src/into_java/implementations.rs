use crate::IntoJava;
use jni::{
    objects::AutoLocal,
    sys::{jboolean, JNI_FALSE, JNI_TRUE},
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

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for String {
    const JNI_SIGNATURE: &'static str = "Ljava/lang/String;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        let jstring = env.new_string(&self).expect("Failed to create Java String");

        env.auto_local(jstring.into())
    }
}
