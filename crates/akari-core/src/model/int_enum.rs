/// Declares an integer enum that keeps values it doesn't know as `Unknown`.
macro_rules! int_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $value:literal,)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)+
            /// A value this version of Akari doesn't know yet.
            Unknown(u16),
        }

        impl From<u16> for $name {
            fn from(value: u16) -> Self {
                match value {
                    $($value => Self::$variant,)+
                    other => Self::Unknown(other),
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                <u16 as serde::Deserialize>::deserialize(deserializer).map(Self::from)
            }
        }
    };
}

pub(crate) use int_enum;

#[cfg(test)]
mod tests {
    int_enum! {
        pub enum Color {
            Red = 0,
            Green = 1,
        }
    }

    #[test]
    fn maps_known_values() {
        assert_eq!(serde_json::from_str::<Color>("1").unwrap(), Color::Green);
    }

    #[test]
    fn keeps_unknown_values() {
        assert_eq!(
            serde_json::from_str::<Color>("42").unwrap(),
            Color::Unknown(42)
        );
    }
}
