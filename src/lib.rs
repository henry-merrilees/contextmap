#![feature(
    entry_insert,
    is_some_and,
    let_chains,
    map_try_insert,
    specialization,
    type_changing_struct_update,
)]
use std::cmp::Ordering;
use std::collections::btree_map::{self, Entry};
use std::collections::hash_map;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{Display, Debug};
use std::hash::Hash;
use std::rc::Rc;

struct Record<C, V> {
    context: C,
    value: V,
}

impl<C: Debug, V: Debug> Debug for Record<C, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Record {context, value} = self;
        write!(f, "Record {{ {value:?} @ {context:?} }}")
    }
}

impl<C, V> From<(C, V)> for Record<C, V> {
    fn from((context, value): (C, V)) -> Self {
        Self { context, value }
    }
}

impl<C: Clone, V: Clone> Clone for Record<C, V> {
    fn clone(&self) -> Self {
        let Self { context, value } = self;
        Self {
            context: context.clone(),
            value: value.clone(),
        }
    }
}
impl<C, V> From<std::collections::btree_map::OccupiedEntry<'_, Rc<C>, Option<Rc<V>>>> for &Record<C, V> {
    fn from(value: std::collections::btree_map::OccupiedEntry<'_, Rc<C>, Option<Rc<V>>>) -> Self {
       
    }
}
impl<C: Ord + Clone, V: Clone> From<btree_map::OccupiedEntry<'_, C, V>> for Record<C, V> {
    fn from(entry: btree_map::OccupiedEntry<C, V>) -> Self {
        Self {
            context: entry.key().clone(),
            value: entry.get().clone(),
        }
    }
}

impl<C, V> From<Record<C, V>> for Record<C, Option<V>> {
    fn from(Record { context, value }: Record<C, V>) -> Self {
        Self {
            context: context.into(),
            value: Some(value.into()),
        }
    }
}

impl<C, V> From<Record<C, V>> for Record<Rc<C>, Rc<V>> {
    fn from(Record { context, value }: Record<C, V>) -> Self {
        Self {
            context: context.into(),
            value: value.into(),
        }
    }
}

struct Registry<C: Ord, V> {
    records: BTreeMap<C, V>,
}

impl<C: Ord, V> Registry<C, V> {
    fn new() -> Self {
        Self {
            records: BTreeMap::new(),
        }
    }

    pub fn get(&self, context: C) -> Option<Record<&C, &V>> {
        self.records.range(..=context).next_back().map(Record::from)
    }

    pub fn get_recent(&self) -> Option<Record<&C, &V>> {
        self.records.last_key_value().map(Into::into)
    }

    pub fn entry(&mut self, context: C) -> Entry<'_, C, V> {
        self.records.entry(context)
    }

    pub fn last_entry(&mut self) -> Option<btree_map::OccupiedEntry<'_, C, V>> {
        self.records.last_entry()
    }
}


impl<C: Ord, V> ContextRegistry<C, V> {

} 


enum InsertionCommand<'a, L, C, V> {
    // new.context == existing.context && new.value.is_some() && existing.value.is_none(None)
    Overwrite {
        link: Rc<L>,
        new_values_to_links_entry: hash_map::Entry<Rc<V>, Rc<C>>
        existing_record: btree_map::OccupiedEntry<'a, Rc<C>, Option<Rc<V>>>,
        new_value: Option<Rc<V>>,
    }, 
    // new.context > existing.context && new.value != existing value
    Update {
        link: Rc<L>,
        new_values_to_links_entry: hash_map::Entry<Rc<V>, Rc<C>>
        new_entry: btree_map::VacantEntry<'a, Rc<C>, Option<Rc<V>>>,
        new_value: Option<Rc<V>>,
        last_value: Option<Rc<V>>,
    },
    // new.value == existing.value
    NoChange,
}

#[derive(Debug)]
enum RecordToEntryError<'a, C, V> {
    OutdatedContext{
        existing_record: &'a Record<C, V>,
        entered_context: &'a C,
    },
    OverwritingSome{
        existing_record: &'a Record<C, V>,
        entered_context: &'a C,
    },
}

const INTRO: &'static str = "Insertions are possible only when the most recent existing record\r\t(1) has less recent context than the record to be inserted, or\r\t(2) is equally recent to the most recent existing record which has a value of None.";

impl<'a, C, V> Display for RecordToEntryError<'a, Rc<C>, Option<Rc<V>>> {
    default fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordToEntryError::OutdatedContext { existing_record, entered_context } => {
                write!(f, "Inserted record must not have an eariler context than the latest existing record.\r\r{INTRO}")
            },
            RecordToEntryError::OverwritingSome { existing_record, entered_context } => {
                write!(f, "Inserted record is equally recent (same context) but value is not None.\r\r{INTRO}")
            },
        }
    }
}

impl<'a, C: Debug, V: Debug> Display for RecordToEntryError<'a, Rc<C>, Option<Rc<V>>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordToEntryError::OutdatedContext { existing_record, entered_context } => {
                write!(f, "Inserted record (@ {entered_context:?}) must not have an eariler context than the latest existing record ({existing_record:?}).\r\r{INTRO}")
            },
            RecordToEntryError::OverwritingSome { existing_record, entered_context } => {
                write!(f, "Inserted record is equally recent (@ {entered_context:?}) to the existing record but its value ({:?}) is not None.\r\r{INTRO}", existing_record.value)
            },
        }
    }
}


impl<'a, C: Debug, V: Debug> std::error::Error for RecordToEntryError<'a, C, V> where Self: Display {}

impl<C: Ord, V> Registry<Rc<C>, Option<Rc<V>>> {
    fn record_to_entry(
        &mut self,
        record: Record<Rc<C>, Option<Rc<V>>>,
    ) -> Result<btree_map::Entry<Rc<C>, Option<Rc<V>>>, RecordToEntryError<C, V>> {
        let last_entry = self
            .last_entry()
            .expect("TODO: there should never be an empty Registry.");

        match (record.context.cmp(last_entry.key()), last_entry.get()) {
        (Ordering::Less, _) => Err(RecordToEntryError::OutdatedContext { existing_record: last_entry.into(), entered_context: &record.context }),
        (Ordering::Equal, &Some(_)) => Err(RecordToEntryError::OverwritingSome { existing_record: last_entry.into(), entered_context: &record.context}),
        _ => Ok(self.entry(record.context)), // All else is okay
        }
    }
}

type ContextRegistry<C, V> = Registry<Rc<C>, Option<Rc<V>>>;

struct ContextMap<L, C: Ord, V> {
    links_to_registries: HashMap<Rc<L>, ContextRegistry<C, V>>,
    values_to_links: HashMap<Rc<V>, Rc<L>>,
}

impl<L, C, V> ContextMap<L, C, V> where L: PartialEq + Eq + Hash, C: Ord, V: Hash + Eq {
    fn new() -> Self {
        let links_to_registries = HashMap::<Rc<L>, ContextRegistry<C, V>>::new();
        let values_to_links = HashMap::<Rc<V>, Rc<L>>::new();
        Self {
            links_to_registries,
            values_to_links,
        }
    }

    pub fn insert<'a>(&mut self, link: L, record: Record<C, V>) -> Result<(), Box<dyn Error>> {
        let record: Record<Rc<C>, Rc<V>> = record.into();
        // Check for different link that points to the same value. We need to write a new None entry
        let overwrite_value_record_result: Result<
            Option<btree_map::Entry<Rc<C>, Option<Rc<V>>>>,
            RecordToEntryError<C, V>,
        > = match self.values_to_links.entry(record.value.clone()) {
            hash_map::Entry::Vacant(_) => Ok(None),
            hash_map::Entry::Occupied(occupied_entry_link) => {
                let key: &Rc<L> = occupied_entry_link.get();
                let mut registry = self.links_to_registries.get_mut(key).expect("If this is reached, a {{link: value}} was not properly added to self.values_to_links.");
                registry.record_to_entry(record.clone().into()).map(Some)
            }
        };

        // Check for records with earlier contexts
        //
        let mut binding: hash_map::OccupiedEntry<Rc<L>, Registry<Rc<C>, Option<Rc<V>>>>;
        let overwrite_link_record_result: Result<btree_map::Entry<Rc<C>, Option<Rc<V>>>, RecordToEntryError<C, V>> =
            match self.links_to_registries.entry(Rc::new(link)) {
                hash_map::Entry::Occupied(ref mut occupied_entry) => {
                    let registry = occupied_entry.get_mut();
                    registry.record_to_entry(record.into())
                }
                hash_map::Entry::Vacant(vacant_entry) => {
                    binding = vacant_entry.insert_entry(Registry::new());
                    Ok(binding.get_mut().entry(record.context))
                }
            };

        match (overwrite_link_record_result, overwrite_value_record_result) {
            (Ok(_), Ok(_)) => todo!(),
            (Ok(_), Err(_)) => todo!(),
            (Err(_), Ok(_)) => todo!(),
            (Err(e1), Err(e2)) => Err(format!("{} and {}", e1, e2).into()),

                Err(e1.to_string())
        }

        // If we write a newer record, we should remove the old value from values_to_links

        // Register new value in values_to_links
    }
}
