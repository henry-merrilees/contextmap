#![feature(map_try_insert)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

struct Registry<C, V>(BTreeMap<C, Option<Rc<V>>>);

impl<C, V> Registry<C, V>
where
    C: Ord + Clone + Debug,
    V: Hash + Eq + Debug,
{
    fn new() -> Self {
        Self(BTreeMap::<C, Option<Rc<V>>>::new())
    }

    fn get(&self, context: &C) -> Option<Rc<V>> {
        self.0
            .range(..=context) // Get all pairs until (inclusive) context
            .next_back() // Take the last one
            .and_then(|(_c, v)| v.clone()) // take the value and upgrade to an owned Rc<V>
    }

    fn update(&mut self, context: C, value: Option<Rc<V>>) -> Result<(), &'static str> {
        // If records already exist, our new context must be the newest!
        if !self.0.is_empty() && self.0.last_key_value().unwrap().0 >= &context {
            return Err("More recent context exists.");
        }
        self.0.try_insert(context, value).unwrap();
        Ok(())
    }
}

pub struct ContextMap<K, C, V>
where
    K: Hash + Eq,
    C: Ord + Clone,
    V: Hash + Eq,
{
    keys_to_registries: HashMap<Rc<K>, Registry<C, V>>,
    values_to_keys: HashMap<Rc<V>, Rc<K>>,
}

impl<K, C, V> ContextMap<K, C, V>
where
    K: Debug + Hash + Eq,
    C: Debug + Ord + Clone,
    V: Debug + Hash + Eq,
{
    pub fn get(&self, key: K, context: &C) -> Option<Rc<V>> {
        self.keys_to_registries
            .get(&key)
            .and_then(|r| r.get(context))
    }

    fn get_registry(&mut self, key: Rc<K>) -> &mut Registry<C, V> {
        if !self.keys_to_registries.contains_key(&key) {
            self.keys_to_registries.insert(key.clone(), Registry::new());
        }
        self.keys_to_registries.get_mut(&key).unwrap()
    }

    pub fn update_overwrite(
        &mut self,
        key: Rc<K>,
        context: C,
        value: Rc<V>,
    ) -> Result<(), &'static str> {
        if let Some(k) = self.values_to_keys.get(&value) {
            let old_registry = self.keys_to_registries.get_mut(k).unwrap();
            old_registry.update(context.clone(), None)?;
            self.values_to_keys.remove(&value);
        }
        self.update_unchecked(key, context, value)
    }

    pub fn update_no_overwrite(
        &mut self,
        key: Rc<K>,
        context: C,
        value: Rc<V>,
    ) -> Result<(), &'static str> {
        if self.values_to_keys.contains_key(&value) {
            return Err("Value has a live Key.");
        }
        self.update_unchecked(key, context, value)
    }

    fn update_unchecked(
        &mut self,
        key: Rc<K>,
        context: C,
        value: Rc<V>,
    ) -> Result<(), &'static str> {
        let new_registry = self.get_registry(key.clone());
        new_registry.update(context, Some(value.clone()))?;
        self.values_to_keys.try_insert(value, key).unwrap();
        Ok(())
    }
}
