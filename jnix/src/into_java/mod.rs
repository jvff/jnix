mod implementations;

use jni::JNIEnv;

pub trait IntoJava<'borrow, 'env: 'borrow> {
    const JNI_SIGNATURE: &'static str;

    type JavaType;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType;
}
