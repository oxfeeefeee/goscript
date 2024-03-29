#[cfg(feature = "btree_map")]
pub type Map<K, V> = std::collections::BTreeMap<K, V>;
#[cfg(not(feature = "btree_map"))]
pub type Map<K, V> = std::collections::HashMap<K, V>;

#[cfg(feature = "btree_map")]
pub type MapIter<'a, K, V> = std::collections::btree_map::Iter<'a, K, V>;
#[cfg(not(feature = "btree_map"))]
pub type MapIter<'a, K, V> = std::collections::hash_map::Iter<'a, K, V>;
