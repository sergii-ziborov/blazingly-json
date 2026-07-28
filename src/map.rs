use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::btree_map;
use std::collections::BTreeMap;
use std::fmt;
use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};

/// Ordered object storage used by [`crate::Value`].
pub struct Map<K, V> {
    values: BTreeMap<K, V>,
}

impl<K: Ord, V> Map<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.values.insert(key, value)
    }

    #[must_use]
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.values.get(key)
    }

    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        self.values.get_mut(key)
    }

    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.values.remove(key)
    }
}

impl Map<String, crate::Value> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub fn entry<S: Into<String>>(&mut self, key: S) -> btree_map::Entry<'_, String, crate::Value> {
        self.values.entry(key.into())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

impl<K: Ord, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }
}

impl<K: Ord + Clone, V: Clone> Clone for Map<K, V> {
    fn clone(&self) -> Self {
        Self {
            values: self.values.clone(),
        }
    }
}

impl<K: Ord + fmt::Debug, V: fmt::Debug> fmt::Debug for Map<K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.values.fmt(formatter)
    }
}

impl<K: Ord + PartialEq, V: PartialEq> PartialEq for Map<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl<K: Ord + Eq, V: Eq> Eq for Map<K, V> {}

impl<K: Ord, V> Deref for Map<K, V> {
    type Target = BTreeMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<K: Ord, V> DerefMut for Map<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            values: BTreeMap::from_iter(iter),
        }
    }
}

impl<K: Ord, V> Extend<(K, V)> for Map<K, V> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        self.values.extend(iter);
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for Map<K, V> {
    fn from(entries: [(K, V); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<K: Ord, V> From<BTreeMap<K, V>> for Map<K, V> {
    fn from(values: BTreeMap<K, V>) -> Self {
        Self { values }
    }
}

impl<K: Ord, V> From<Map<K, V>> for BTreeMap<K, V> {
    fn from(map: Map<K, V>) -> Self {
        map.values
    }
}

impl<K: Ord, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = btree_map::IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl<'a, K: Ord, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = btree_map::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<'a, K: Ord, V> IntoIterator for &'a mut Map<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = btree_map::IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter_mut()
    }
}

impl<K: Ord + Serialize, V: Serialize> Serialize for Map<K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.values.serialize(serializer)
    }
}

impl<'de, K: Ord + Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for Map<K, V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::deserialize(deserializer).map(Self::from)
    }
}
