use crate::IntoJava;
use jni::{objects::AutoLocal, JNIEnv};

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for String {
    const JNI_SIGNATURE: &'static str = "Ljava/lang/String;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        let jstring = env.new_string(&self).expect("Failed to create Java String");

        env.auto_local(jstring.into())
    }
}
