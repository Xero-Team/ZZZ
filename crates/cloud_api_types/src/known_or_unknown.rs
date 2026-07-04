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
    use crate::Plan;

    #[test]
    fn deserializes_known_and_unknown_values() {
        let known = serde_json::from_str::<KnownOrUnknown<Plan, String>>("\"zed_pro\"").unwrap();
        assert_eq!(known, KnownOrUnknown::Known(Plan::ZedPro));

        let unknown =
            serde_json::from_str::<KnownOrUnknown<Plan, String>>("\"enterprise_plus\"").unwrap();
        assert_eq!(
            unknown,
            KnownOrUnknown::Unknown("enterprise_plus".to_string())
        );
    }
}
