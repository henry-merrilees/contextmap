#![feature(map_try_insert)]
use std::collections::hash_map::OccupiedEntry;
use std::collections::{BTreeMap, HashMap, btree_map::OccupiedEntry};
use std::rc::Rc;

struct Record<C, V> {
    context: C,
    value: V,
}

impl<C, V, A> From<OccupiedEntry<'static, C, V>> for Record<C, V> {
    fn from(OccupiedEntry{ base }: OccupiedEntry) -> Self {
        unimplemented!()
    }
}

impl<C, V> From<(C, V)> for Record<C, V> {
    fn from((context, value): (C, V)) -> Self {
        Self { context, value }
    }
    
}

impl<C, V> Record<C, V> {
    fn new(context: C, value: V) -> Self {
        Self {context, value}
    }
}

struct Registry<C: Ord, V> {
    records: BTreeMap<C, V>,
}

impl<C: Ord, V> Registry<C, V> {
    pub fn get(&self, context: C) -> Option<Record<&C, &V>> {
        self.records.range(..context).next_back().map(Record::from)
    }

    pub fn get_recent(&self, context: C) -> Option<Record<&C, &V>> {
        self.records.last_key_value().map(Into::into)
    }

    fn new(record: Record<C, V>) -> Self {
        let records = BTreeMap::<C, V>::new();
        let mut registry = Self { records };
        registry.insert(record);
        registry
    }

    fn insert(&mut self, record: Record<C, V>) {
        self.records.insert(record.context, record.value);
    }
}

type ContextRegistry<C, V> = Registry<C, Option<Rc<V>>>;

struct ContextMap<L, C: Ord, V> {
    links_to_registries: HashMap<L, ContextRegistry<C, V>>,
    values_to_links: HashMap<L, Rc<V>>
}

impl<L, C: Ord, V> ContextMap<L, C, V> {
    fn new() -> Self {
        let links_to_registries = HashMap::<L, ContextRegistry<C, V>>::new();
        let values_to_links = HashMap::<L, Rc<V>>::new();
        Self {
            links_to_registries,
            values_to_links,
        }
    }

    fn insert(link: L, record: Record<C, V>)
}
