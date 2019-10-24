mod implementations;

use crate::AsJValue;
use jni::JNIEnv;

pub trait IntoJava<'borrow, 'env: 'borrow> {
    const JNI_SIGNATURE: &'static str;

    type JavaType: AsJValue<'env>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType;

    fn jni_signature(&self) -> &'static str {
        Self::JNI_SIGNATURE
    }
}
