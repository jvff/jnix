use crate::{AsJValue, IntoJava};
use jni::{
    objects::{AutoLocal, JObject, JValue},
    signature::JavaType,
    sys::{jboolean, jdouble, jint, JNI_FALSE, JNI_TRUE},
    JNIEnv,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for i32 {
    const JNI_SIGNATURE: &'static str = "I";

    type JavaType = jint;

    fn into_java(self, _: &'borrow JNIEnv<'env>) -> Self::JavaType {
        self as jint
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

fn ipvx_addr_into_java<'borrow, 'env: 'borrow>(
    original_octets: &[u8],
    env: &'borrow JNIEnv<'env>,
) -> AutoLocal<'env, 'borrow> {
    let constructor = env
        .get_static_method_id(
            "java/net/InetAddress",
            "getByAddress",
            "([B)Ljava/net/InetAddress;",
        )
        .expect("Failed to get InetAddress.getByAddress method ID");

    let octets_array = env
        .new_byte_array(original_octets.len() as i32)
        .expect("Failed to create byte array to store IP address");

    let octet_data: Vec<i8> = original_octets
        .into_iter()
        .map(|octet| *octet as i8)
        .collect();

    env.set_byte_array_region(octets_array, 0, &octet_data)
        .expect("Failed to copy IP address octets to byte array");

    let octets = env.auto_local(JObject::from(octets_array));
    let result = env
        .call_static_method_unchecked(
            "java/net/InetAddress",
            constructor,
            JavaType::Object("java/net/InetAddress".to_owned()),
            &[octets.as_jvalue()],
        )
        .expect("Failed to create InetAddress Java object");

    match result {
        JValue::Object(object) => env.auto_local(object),
        value => {
            panic!(
                "InetAddress.getByAddress returned an invalid value: {:?}",
                value
            );
        }
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for Ipv4Addr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetAddress;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        ipvx_addr_into_java(self.octets().as_ref(), env)
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for Ipv6Addr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetAddress;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        ipvx_addr_into_java(self.octets().as_ref(), env)
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for IpAddr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetAddress;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JNIEnv<'env>) -> Self::JavaType {
        match self {
            IpAddr::V4(address) => address.into_java(env),
            IpAddr::V6(address) => address.into_java(env),
        }
    }
}
