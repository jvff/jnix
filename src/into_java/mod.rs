use jni::JNIEnv;

pub trait IntoJava<'borrow, 'env: 'borrow> {
    type JavaType;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType;
}
