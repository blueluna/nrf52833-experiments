/// Creates an enum with various traits.
/// The first key-value pair is the default used if any conversion would fail.
#[macro_export]
macro_rules! extended_enum {
    ($(#[$outer:meta])* $name:ident, $ty:ty, $($(#[$inner:meta])* $var:ident => $val:expr),+ $(,)*) => (

        $(#[$outer])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub enum $name {
            $(
                $(#[$inner])*
                $var,
            )*
        }

        impl From<$name> for $ty {
            fn from(v: $name) -> Self {
                match v {
                    $( $name::$var => $val, )*
                }
            }
        }

        impl PartialEq<$name> for $ty {
            fn eq(&self, other: &$name) -> bool {
                match *other {
                    $( $name::$var => *self == $val, )*
                }
            }
        }
    );
}