macro_rules! basic_type {
    ($(#[$attr:meta])* $name:ident, $type:ty) => {
        $(#[$attr])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord
        )]
        pub struct $name(pub $type);

        impl $name {
            pub fn next(self) -> $name {
                $name(self.0 + 1)
            }
        }

        impl Deref for $name {
            type Target = $type;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl FromStr for $name {
            type Err = ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let value = s.parse::<$type>()?;
                Ok(Self(value))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Add<$type> for $name {
            type Output = Self;

            fn add(self, other: $type) -> Self {
                Self(self.0 + other)
            }
        }

        impl std::ops::AddAssign<$type> for $name {
            fn add_assign(&mut self, other: $type) {
                self.0 += other;
            }
        }

        impl Sub<$type> for $name {
            type Output = Self;

            fn sub(self, other: $type) -> Self {
                Self(self.0 - other)
            }
        }

        impl std::ops::SubAssign<$type> for $name {
            fn sub_assign(&mut self, other: $type) {
                self.0 -= other;
            }
        }

        impl From<$type> for $name {
            fn from(value: $type) -> Self {
                Self(value)
            }
        }
    };
}

/// Shortcuts both error variants in a `Result<_, OrStopped>`.
///
/// The [`Internal`](crate::OrStopped::Internal) variant is mapped to an error, and the [`Stopped`](crate::OrStopped::Stopped)
/// variant is mapped to `Ok(())`. As such, a method / function where this macro is invoked must return `anyhow::Result<()>`.
///
/// This macro makes sense to call in higher-level tasks that compose lower-level code returning `Result<_, OrStopped>`.
#[macro_export]
macro_rules! try_stoppable {
    ($res:expr) => {
        match $res {
            ::core::result::Result::Ok(value) => value,
            ::core::result::Result::Err($crate::OrStopped::Stopped) => {
                ::tracing::info!("Stop request was received");
                return Ok(());
            }
            ::core::result::Result::Err($crate::OrStopped::Internal(err)) => {
                #[allow(clippy::useless_conversion)]
                return Err(err.into());
            }
        }
    };
}
