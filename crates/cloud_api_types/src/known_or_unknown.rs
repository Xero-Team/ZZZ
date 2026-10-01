use serde::{Deserialize, Serialize};

/// `KnownOrUnknown` is a type that represents either a known value ([`Known`](KnownOrUnknown::Known))
/// or an unknown value ([`Unknown`](KnownOrUnknown::Unknown)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KnownOrUnknown<K, U> {
    /// A known value.
    Known(K),
    /// An unknown value.
    Unknown(U),
}

#[cfg(test)]
mod tests {
    use super::KnownOrUnknown;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum KnownValue {
        Known,
    }

    #[test]
    fn deserializes_known_and_unknown_values() {
        let known = serde_json::from_str::<KnownOrUnknown<KnownValue, String>>("\"known\"")
            .expect("known value should deserialize");
        assert_eq!(known, KnownOrUnknown::Known(KnownValue::Known));

        let unknown =
            serde_json::from_str::<KnownOrUnknown<KnownValue, String>>("\"enterprise_plus\"")
                .expect("unknown value should deserialize through the fallback type");
        assert_eq!(
            unknown,
            KnownOrUnknown::Unknown("enterprise_plus".to_string())
        );
    }
}
