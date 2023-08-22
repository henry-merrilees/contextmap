//! Contextmap is a crate which provides `ContextMap`, which relates link-value pairs at particular
//! contexts. 
//!
//!

#![warn(missing_docs, missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![feature(
    map_many_mut,
    map_entry_replace,
    entry_insert,
    is_some_and,
    let_chains,
    map_try_insert,
    type_changing_struct_update
)]

//! Context map relates links to values at given (orderable) contexts. At any given context,
//! `ContextMap` relates links to values as a [partial bijection](https://en.wikipedia.org/wiki/Bijection#Generalization_to_partial_functions).

use core::fmt;
use std::cell::RefCell;
use std::cmp::{Ord, Ordering};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::RangeToInclusive;
use std::rc::Rc;

/// The core unit of a hashmap
#[derive(Debug)]
pub struct ContextRecord<C: Ord, L, V> {
    context: Rc<C>,
    link: Rc<L>,
    value: Option<Rc<V>>,
}

impl<C: Ord, L, V> Clone for ContextRecord<C, L, V> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            link: self.link.clone(),
            value: self.value.clone(),
        }
    }
}

impl<C: Ord, L, V> ContextRecord<C, L, V> {
    /// Create a new record with a `Some` value
    fn new_some(context: &Rc<C>, link: &Rc<L>, value: &Rc<V>) -> Self {
        let context = context.clone();
        let link = link.clone();
        let value = value.clone();

        Self {
            context,
            link,
            value: Some(value),
        }
    }

    /// Create a new record with a `None` value
    fn new_none(context: &Rc<C>, link: &Rc<L>) -> Self {
        let context = context.clone();
        let link = link.clone();
        Self {
            context,
            link,
            value: None,
        }
    }
}

#[derive(Debug)]
struct ContextRegistry<C: Ord, L, V> {
    records: BTreeMap<Rc<C>, ContextRecord<C, L, V>>,
}

impl<C: Ord, L, V> ContextRegistry<C, L, V> {
    pub fn insert(
        &mut self,
        context: Rc<C>,
        record: ContextRecord<C, L, V>,
    ) -> Option<ContextRecord<C, L, V>> {
        self.records.insert(context, record)
    }

    pub fn query(&self, context: &C) -> Option<&ContextRecord<C, L, V>> {
        self.records
            .range::<C, RangeToInclusive<&C>>(..=context)
            .next_back()
            .map(|(_c, v)| v)
    }

    fn get_mut(&mut self, context: &C) -> Option<&mut ContextRecord<C, L, V>> {
        self.records.get_mut(context)
    }

    fn new(context: Rc<C>, link: Rc<L>, value: Rc<V>) -> Self {
        let record = ContextRecord::new_some(&context, &link, &value);
        let mut records = BTreeMap::new();
        records.insert(context, record);

        Self { records }
    }

    fn last_key_value(&self) -> (&Rc<C>, &ContextRecord<C, L, V>) {
        self.records
            .last_key_value()
            .expect("Registries are not created without records.")
    }
    fn last_record(&self) -> &ContextRecord<C, L, V> {
        self.last_key_value().1
    }

    fn context(&self) -> Rc<C> {
        self.last_record().context.clone()
    }

    fn link(&self) -> Rc<L> {
        self.last_record().link.clone()
    }

    fn value(&self) -> Option<Rc<V>> {
        self.last_record().value.clone()
    }
}

//  Rules:
//  - Records:
//    - A context, link and an optional value.
//  - Registries:
//    - An ordered map of records.
//    - The "value" of a record is the value of its most recent record (a None-valued record
//    means a none-valued registry)
//    - Write-only
//    - Records are inserted in chronological order
//
//  - ContextMap:
//    - Points links to registries
//    - Points values to the registries of the same ("live") value.
//    - No two registries may have the same link.
//    - No two registries may have the same value.
//    - Registries should be inserted into the contextmap only with a single some-valued record;
//    this way the insertion logic of the registry follows the insertion logic of its record.
//
//  Operations:
//  - No change: submitting a record with this link and value would not change the outputs of the
//  map for any context.
//  - NewLink: the link has never existed in the map.
//  - Nullify: enters a None record at a more recent context. (Enables an overwrite/update)
//  - Overwrite: replaces the last record's None with some value.
//  - Update: Writes a new Some value at a more recent context.
//
//  - the link-to-registry and value-to-registry entries must not both contain the same registry.
//  If we can ensure this rule, we can borrow mutably from either without worry that we have
//  already mutably borrowed the same RefCell from the other.
//
//
//
//  Guarantees:
//  - Registries:
//    - After a record has been inserted at a context, any query context not later than the last
//    inserted context will return the same value (bc write-only & chronological insertion rule).
//    - For space efficiency, we can regard an inserted record with the same value as the last
//    record as a no-op without erroring, (as query returns most recent, it does not change result
//    for any context)
//  - Update:
//    - the link_
//
#[derive(Debug)]
pub struct ContextMap<L, C: Ord, V> {
    links_to_registries: HashMap<Rc<L>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
    values_to_registries: HashMap<Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
}

/// ## InsertionCommands are determined by the following:
///
/// Link does not exist: `NewLink` (`Some(value)`, any context)
///
/// Link Exists:  
///
/// | ↓ value / context →         | Same Context                               | Later Context                          |
/// |-----------------------------|--------------------------------------------|----------------------------------------|
/// | `None` -> `Some(new_value)` | Overwrite                                  | Update                                 |
/// | `Some(old_value)` -> `None` | N/A (Destructive)                          | Nullify                                |
///
///
#[derive(Debug)]
pub enum LinkInsertionCommand<C, L, V> {
    NewLink {
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    },
    Update {
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    },
    Overwrite {
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    },
    NoChange,
}

#[derive(Debug)]
pub enum ValueInsertionCommand<C, V> {
    AddValue {
        new_value: Rc<V>,
    },
    RemoveExistingValueAddNewValue {
        existing_value: Rc<V>,
        new_context: Rc<C>,
        new_value: Rc<V>,
    },
}

#[derive(Debug)]
pub enum InsertionError {
    OutdatedContext,
    OverwritingSome,
    NullifyingSome,
}

impl fmt::Display for InsertionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for InsertionError {}


impl<L, C, V> ContextMap<L, C, V>
where
    L: PartialEq + Eq + Hash,
    C: Ord + Debug,
    V: Hash + Eq + Debug,
{
    pub fn new() -> Self {
        Self {
            links_to_registries: HashMap::<Rc<L>, Rc<RefCell<ContextRegistry<C, L, V>>>>::new(),
            values_to_registries: HashMap::<Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>::new(),
        }
    }

    pub fn query(&self, context: &C, link: &L) -> Option<ContextRecord<C, L, V>> {
        self.links_to_registries
            .get(link)
            .and_then(|r| r.borrow().query(context).cloned())
    }

    pub fn get_live_value(&self, link: &Rc<L>) -> Option<Rc<V>> {
        self.links_to_registries
            .get(link)
            .map(|registry| registry.borrow().value())
            .flatten()
    }
    pub fn get_live_link(&self, value: &Rc<V>) -> Option<Rc<L>> {
        self.values_to_registries
            .get(value)
            .map(|registry| registry.borrow().link())
    }

    // TODO: Get rid of this
    pub fn insert(
        &mut self,
        context: impl Into<Rc<C>>,
        link: impl Into<Rc<L>>,
        value: impl Into<Rc<V>>,
    ) -> Result<(), Box<dyn Error>> {
        self.insert_with_overwrite(context.into(), link.into(), value.into()).map_err(|e| e.into())
    }

    pub fn insert_without_overwrite(
        &mut self,
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    ) -> Result<(), Box<dyn Error>> {
        let commands = self.generate_insertion_commands(&context, &link, &value)?;
        match commands {
            (LinkInsertionCommand::Overwrite { .. }, _) => return Err("Overwrite Error. TODO".into()),
            (link_command, value_command) => {
                self.execute_insertion_commands(link_command, value_command).map_err(|e| e.into())
            }
        }
    }

    pub fn insert_with_overwrite(&mut self, context: Rc<C>, link: Rc<L>, value: Rc<V>) -> Result<(), InsertionError> {
        let (link_command, value_command) = self.generate_insertion_commands(&context, &link, &value)?;
        self.execute_insertion_commands(link_command, value_command)
    }

    fn execute_insertion_commands(&mut self, link_command: LinkInsertionCommand<C, L, V>, value_command: ValueInsertionCommand<C, V>) -> Result<(), InsertionError> {
        if let Some(new_registry) = self.execute_link_insertion_command(link_command) {
            Ok(self.execute_value_insertion_command(value_command, new_registry))
        } else {
            Ok(())
        }
    }

    fn generate_insertion_commands(&self, context: &Rc<C>, link: &Rc<L>, value: &Rc<V>) -> Result<(LinkInsertionCommand<C, L, V>, ValueInsertionCommand<C, V>), InsertionError> {
        let link_command = self.generate_link_insertion_command(context, link, value)?;
        let value_command = self.generate_value_insertion_command(context, link, value)?;

        Ok((link_command, value_command))
    }

    fn generate_value_insertion_command(
        &self,
        context: &Rc<C>,
        link: &Rc<L>,
        value: &Rc<V>,
    ) -> Result<ValueInsertionCommand<C, V>, InsertionError> {
        let registry_with_value = match self.values_to_registries.get(value) {
            Some(registry) => registry.clone(),
            None => {
                return Ok(ValueInsertionCommand::AddValue {
                    new_value: value.clone(),
                })
            }
        };

        let existing_value_context = registry_with_value.borrow().context();

        // If there is a value registry to be nullified (i.e. Some(_)), the overwriting context
        // must not be less than or equal to the existing context
        match context.cmp(&existing_value_context) {
            Ordering::Less => return Err(InsertionError::OutdatedContext),
            Ordering::Equal => {
                // Valued_registries are necessarily some-valued, so we would be nullifying Some.
                return Err(InsertionError::NullifyingSome);
            }
            Ordering::Greater => Ok(ValueInsertionCommand::RemoveExistingValueAddNewValue {
                existing_value: self
                    .get_live_value(link)
                    .expect("Nullifying implies value to be nullified"),
                new_context: context.clone(),
                new_value: value.clone(),
            }),
        }
    }

    fn execute_value_insertion_command(
        &mut self,
        command: ValueInsertionCommand<C, V>,
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>,
    ) {
        match command {
            ValueInsertionCommand::AddValue { new_value } => {
                self.values_to_registries.insert(new_value, new_registry);
            }
            ValueInsertionCommand::RemoveExistingValueAddNewValue {
                existing_value,
                new_value,
                new_context,
            } => {
                let existing_registry = self.values_to_registries.remove(&existing_value).unwrap();
                let existing_link = existing_registry.borrow().link();
                let null_record = ContextRecord::new_none(&new_context, &existing_link);
                existing_registry
                    .borrow_mut()
                    .insert(new_context, null_record);
                self.values_to_registries.insert(new_value, new_registry);
            }
        };
    }

    fn generate_link_insertion_command(
        &self,
        context: &Rc<C>,
        link: &Rc<L>,
        value: &Rc<V>,
    ) -> Result<LinkInsertionCommand<C, L, V>, InsertionError> {
        // The link and value already point to the same registry, they are already associated.
        if let (Some(link_registry), Some(value_registry)) = (self.links_to_registries.get(link), self.values_to_registries.get(value)) 
            && std::ptr::eq(link_registry, value_registry) {
            return Ok(LinkInsertionCommand::NoChange);
        }

        let linked_registry = match self.links_to_registries.get(link) {
            Some(linked_registry) => linked_registry,
            None => return Ok(LinkInsertionCommand::NewLink {
                context: context.clone(),
                link: link.clone(),
                value: value.clone(),
            }),
        };

        let linked_context = linked_registry.borrow().context().clone();
        match context.cmp(&linked_context) {
            Ordering::Less => Err(InsertionError::OutdatedContext),
            Ordering::Equal => Ok(LinkInsertionCommand::Overwrite {
                context: context.clone(),
                link: link.clone(),
                value: value.clone(),
            }),
            Ordering::Greater => {
                // Update
                Ok(LinkInsertionCommand::Update {
                    context: context.clone(),
                    link: link.clone(),
                    value: value.clone(),
                })
            }
        }
    }

    fn execute_link_insertion_command(&mut self, command: LinkInsertionCommand<C, L, V>) -> Option<Rc<RefCell<ContextRegistry<C, L, V>>>> {
        match command {
            LinkInsertionCommand::NewLink {
                context,
                link,
                value,
            } => {
                let new_registry = Rc::new(RefCell::new(ContextRegistry::new(
                    context.clone(),
                    link.clone(),
                    value.clone(),
                )));
                self.links_to_registries
                    .insert(link.clone(), new_registry.clone());

                Some(new_registry)
            }
            LinkInsertionCommand::Update {
                context,
                link,
                value,
            } => {
                let existing_registry = self.links_to_registries.get(&link).unwrap();
                let new_record = ContextRecord::new_some(&context, &link, &value);
                existing_registry
                    .borrow_mut()
                    .insert(context.clone(), new_record);

                Some(existing_registry.clone())
            }
            LinkInsertionCommand::Overwrite {
                link,
                context,
                value,
            } => {
                let existing_registry = self.links_to_registries.get(&link).unwrap();
                let mut existing_registry_mut = existing_registry.borrow_mut();
                let mut existing_record = existing_registry_mut.get_mut(&context).unwrap();
                existing_record.value = Some(value.clone());

                Some(existing_registry.clone())
            }
            LinkInsertionCommand::NoChange => None
        }
    }
}

#[test]
fn insert_test() {
    let mut context_map = ContextMap::<u32, u32, u32>::new();
    context_map.insert(0, 0, 0).unwrap();
    dbg!(&context_map);
    context_map.insert(1, 1, 1).unwrap();
    dbg!(&context_map);
    context_map.insert(2, 1, 0).unwrap();
    dbg!(&context_map);

    dbg!(context_map.query(&0, &0));
    dbg!(context_map.query(&1, &0));
    dbg!(context_map.query(&1, &0));

    dbg!(context_map.query(&0, &1));
    dbg!(context_map.query(&1, &1));
    dbg!(context_map.query(&1, &1));
}
