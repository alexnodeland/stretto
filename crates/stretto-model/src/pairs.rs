//! Maps whose keys are not strings, serialized as lists of `[key, value]`
//! pairs sorted by key, so they survive JSON and diff cleanly.
//!
//! Use it as `#[serde(with = "stretto_model::pairs")]` on a map field whose
//! keys are tuples, vectors or numbers.

use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// Serialize `map` as its entries, sorted by key.
pub fn serialize<'a, M, K, V, S>(map: &'a M, s: S) -> Result<S::Ok, S::Error>
where
    &'a M: IntoIterator<Item = (&'a K, &'a V)>,
    K: Serialize + Ord + 'a,
    V: Serialize + 'a,
    S: Serializer,
{
    let mut entries: Vec<(&K, &V)> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    s.collect_seq(entries)
}

/// Rebuild a map from its entries.
pub fn deserialize<'de, M, K, V, D>(d: D) -> Result<M, D::Error>
where
    M: FromIterator<(K, V)>,
    K: Deserialize<'de>,
    V: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Vec::<(K, V)>::deserialize(d)?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "crate::pairs")]
        map: HashMap<(u32, Vec<u32>), f64>,
    }

    #[test]
    fn round_trips_in_key_order() {
        let h = Holder {
            map: HashMap::from([((2, vec![1]), 0.5), ((1, vec![3, 4]), 2.0)]),
        };
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, r#"{"map":[[[1,[3,4]],2.0],[[2,[1]],0.5]]}"#);
        assert_eq!(serde_json::from_str::<Holder>(&json).unwrap(), h);
    }
}
