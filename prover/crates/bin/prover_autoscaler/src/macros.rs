macro_rules! string_type {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(
            Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl FromStr for $name {
            type Err = ParseAError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_owned()))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl $name {
            pub fn new<T: ToString>(name: T) -> Self {
                Self(name.to_string())
            }

            pub fn to_str(&self) -> &str {
                &self.0
            }
        }

        impl EncodeLabelValue for $name {
            fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
                EncodeLabelValue::encode(&self.0.as_str(), encoder)
            }
        }
    };
}
