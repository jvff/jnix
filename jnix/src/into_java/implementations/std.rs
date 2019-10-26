use crate::{AsJValue, IntoJava, JnixEnv};
use jni::{
    objects::{AutoLocal, JList, JObject, JValue},
    signature::JavaType,
    sys::{jboolean, jdouble, jint, jshort, jsize, JNI_FALSE, JNI_TRUE},
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

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

macro_rules! impl_into_java_for_array {
    ($element_type:ty) => {
        impl_into_java_for_array!(
            $element_type,
             0  1  2  3  4  5  6  7  8  9
            10 11 12 13 14 15 16 17 18 19
            20 21 22 23 24 25 26 27 28 29
            30 31 32 33 34 35 36 37 38 39
        );
    };

    ($element_type:ty, $( $count:tt )*) => {
        $(
            impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for [$element_type; $count] {
                const JNI_SIGNATURE: &'static str = "[B";

                type JavaType = AutoLocal<'env, 'borrow>;

                fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
                    (&self as &[$element_type]).into_java(env)
                }
            }
        )*
    };
}

impl_into_java_for_array!(u8);

impl<'borrow, 'env, T> IntoJava<'borrow, 'env> for Option<T>
where
    'env: 'borrow,
    T: IntoJava<'borrow, 'env, JavaType = AutoLocal<'env, 'borrow>>,
{
    const JNI_SIGNATURE: &'static str = T::JNI_SIGNATURE;

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        match self {
            Some(t) => t.into_java(env),
            None => env.auto_local(JObject::null()),
        }
    }
}

impl<'borrow, 'env, T> IntoJava<'borrow, 'env> for Vec<T>
where
    'env: 'borrow,
    T: IntoJava<'borrow, 'env, JavaType = AutoLocal<'env, 'borrow>>,
{
    const JNI_SIGNATURE: &'static str = "Ljava/util/ArrayList;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        let initial_capacity = self.len();
        let parameters = [JValue::Int(initial_capacity as jint)];

        let class = env.get_class("java/util/ArrayList");
        let list_object = env
            .new_object(&class, "(I)V", &parameters)
            .expect("Failed to create ArrayList object");

        let list =
            JList::from_env(env, list_object).expect("Failed to create JList from ArrayList");

        for element in self {
            list.add(element.into_java(env).as_obj())
                .expect("Failed to add element to ArrayList");
        }

        env.auto_local(list_object)
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for String {
    const JNI_SIGNATURE: &'static str = "Ljava/lang/String;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        let jstring = env.new_string(&self).expect("Failed to create Java String");

        env.auto_local(jstring.into())
    }
}

fn ipvx_addr_into_java<'borrow, 'env: 'borrow>(
    original_octets: &[u8],
    env: &'borrow JnixEnv<'env>,
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

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        ipvx_addr_into_java(self.octets().as_ref(), env)
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for Ipv6Addr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetAddress;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        ipvx_addr_into_java(self.octets().as_ref(), env)
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for IpAddr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetAddress;";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        match self {
            IpAddr::V4(address) => address.into_java(env),
            IpAddr::V6(address) => address.into_java(env),
        }
    }
}

impl<'borrow, 'env: 'borrow> IntoJava<'borrow, 'env> for SocketAddr {
    const JNI_SIGNATURE: &'static str = "Ljava/net/InetSocketAddress";

    type JavaType = AutoLocal<'env, 'borrow>;

    fn into_java(self, env: &'borrow JnixEnv<'env>) -> Self::JavaType {
        let ip_address = self.ip().into_java(env);
        let port = self.port() as jint;
        let parameters = [JValue::Object(ip_address.as_obj()), JValue::Int(port)];

        let class = env.get_class("java/net/InetAddress");
        let object = env
            .new_object(&class, "(Ljava/net/InetAddress;I)V", &parameters)
            .expect("Failed to convert SocketAddr Rust type into InetSocketAddress Java object");

        env.auto_local(object)
    }
}
