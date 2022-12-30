#![feature(
    map_many_mut,
    map_entry_replace,
    entry_insert,
    is_some_and,
    let_chains,
    map_try_insert,
    type_changing_struct_update
)]
use std::cell::RefCell;
use std::cmp::{Ord, Ordering};
use std::collections::hash_map;
use std::collections::{BTreeMap, HashMap};
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
        self.records.range::<C, RangeToInclusive<&C>>(..=context).next_back().map(|(_c, v)| v)
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
/// Link does not exist: [`NewLink`](InsertionCommand::NewLink) (`Some(value)`, any context)
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
pub enum InsertionOk {
    NewLink,
    Update,
    Nullify,
    Overwrite,
    NoChange,
}

#[derive(Debug)]
pub enum ValueUpdateOk<C, L, V> {
    NoExistingRegistry {
        new_value: Some<Rc<V>>,
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>
    },
    NullifyWith {
        existing_value: Some<Rc<V>>,
        new_value: Some<Rc<V>>,
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>
    }
}

// trait Command where Self: Sized{
//     fn execute<C: Ord, L, V>(self, &mut context_map: ContextMap<C, L, V>) {}
// }
//
// impl<V> Command for ValueUpdateOk<V> {
//     fn execute<C: Ord, L, V>(self, &mut context_map: ContextMap<C, L, V>) {
//         match
//     }
// }
//


#[derive(Debug)]
pub enum InsertionError {
    OutdatedContext,
    OverwritingSome,
    NullifyingSome,
}

use hash_map::Entry::{Occupied, Vacant};
use std::collections::hash_map::{Entry, VacantEntry};

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
        self.links_to_registries.get(link).and_then(|r| r.borrow().query(context).cloned())
    }

    pub fn contains_live_link(&self, link: &Rc<L>) -> bool {
        self.links_to_registries.contains_key(link)
    }
    pub fn contains_live_value(&self, value: &Rc<V>) -> bool {
        self.values_to_registries.contains_key(value)
    }


    pub fn insert(
        &mut self,
        context: C,
        link: L,
        value: V,
    ) -> Result<InsertionOk, InsertionError> {
        self.try_insert_rc(context.into(), link.into(), value.into())
    }

    fn execute_value_map_update_command(&mut self, command: ValueUpdateOk<V>) {
        match command {
            ValueUpdateOk::NoExistingRegistry {  } => {
            }
            ValueUpdateOk::NullifyWith { existing_value, new_value } => {
                self.val
            }
        }

    }

    fn generate_value_map_update_command(&self, context: &Rc<C>, link: &Rc<L>, value: Option<&Rc<V>>) -> Result<ValueUpdateOk, InsertionError> {
        let existing_registry = match self.values_to_registries.get(value) {
            Some(registry) => {
                registry.clone()
            }
            None => {
                return Ok(ValueUpdateOk::NoExistingRegistry);
            }
        };
        let borrow = registry.borrow();
        let existing_context = registry.borrow().context();
        let existing_value = registry.borrow().value().clone();

        match context.cmp(existing_context) {
            Ordering::Less => {
                Err(InsertionError::OutdatedContext)
            }
            Ordering::Equal => {
                Err(InsertionError::OverwritingSome)
            }
            Ordering::Greater => {
                Ok(ValueUpdateOk::NullifyWith {
                    existing_value,
                    new_value: value.clone(),
                })
            }
        }
    }

    fn try_insert_rc(
        &mut self,
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    ) -> Result<InsertionOk, InsertionError> {
        {
            // The link and value already point to the same registry, they are already associated.
            if let (Some(link_registry), Some(value_registry)) = (self.links_to_registries.get(&link), self.values_to_registries.get(&value)) && std::ptr::eq(link_registry, value_registry) {
                return Ok(InsertionOk::NoChange);
            }

            // If there is a value registry to be nullified (i.e. Some(_)), the overwriting context
            // must not be less than or equal to the existing context
            match valued_context.as_ref().map(|lc| context.cmp(lc)) {
                Some(Ordering::Less) => return Err(InsertionError::OutdatedContext),
                // Valued_registries are necessarily some-valued, so we would be nullifying Some.
                Some(Ordering::Equal) => return Err(InsertionError::NullifyingSome),
                _ => {}
            }
        }

        // Handle linked


        let linked_registry_option = self.links_to_registries.get(&link);
        if linked_registry_option == None {
            self.unchecked_new_link(&context, &link, &value);
            return Ok(InsertionOk::NewLink);
        };
        let linked_registry = linked_registry_option.unwrap();

        let linked_context = linked_registry.borrow().context().clone();
        match context.cmp(&linked_context) {
            Ordering::Less => {
                Err(InsertionError::OutdatedContext)
            }
            Ordering::Equal => {
                self.overwrite_unchecked(&context, &link, &value)
            }
            Ordering::Greater => {
                // Update
                self.unchecked_update(&context, &link, &value)
            }
        }
    }

    fn overwrite_unchecked(&mut self, context: &Rc<C>, link: &Rc<L>, value: &Rc<V>) -> Result<InsertionOk, InsertionError> {
        let linked_registry = self.links_to_registries.get(&link).unwrap();
        let mut linked_registry_mut = linked_registry.borrow_mut();
        let linked_record = linked_registry_mut
            .get_mut(&context)
            .expect("Equal cmp means there is a record at this context");
        if linked_record.value.is_none() {
            // Overwrite
            linked_record.value = Some(value.clone());

            // Record that this value points to the registry which now contains it
            self.values_to_registries
                .insert(value.clone(), linked_registry.clone());

            Ok(InsertionOk::NewLink)
        } else {
            Err(InsertionError::OverwritingSome)
        }
    }

    fn unchecked_new_link(&mut self, context: &Rc<C>, link: &Rc<L>, value: &Rc<V>) {
        let entry = self.values_to_registries.entry(value.clone());
        // NewLink
        let new_linked_registry = Rc::new(RefCell::new(ContextRegistry::new(
            context.clone(),
            link.clone(),
            value.clone(),
        )));
        vacant_links_to_registries_entry.insert(new_linked_registry.clone());
        match entry {
            Occupied(old_values_to_registries_entry) => {
                // Point the value to the new registry, and get the old registry out...
                let old_valued_registry = old_values_to_registries_entry
                    .replace_entry(new_linked_registry)
                    .1;

                // So we can nullify it
                old_valued_registry
                    .borrow_mut()
                    .insert(context.clone(), ContextRecord::new_none(&&&context, &&&link));
            }
            Vacant(valued_registry_vacant_entry) => {
                // Point the value to the
                valued_registry_vacant_entry.insert(new_linked_registry);
            }
        }
    }

    fn unchecked_update(&mut self, context: &Rc<C>, link: &Rc<L>, value: &Rc<V>) -> Result<InsertionOk, InsertionError> {
        let linked_registry = self.links_to_registries.get(link).unwrap();
        let new_record = ContextRecord::new_some(&context, &link, &value);
        let old_value_option = linked_registry.borrow_mut().insert(context.clone(), new_record).value.clone();
        if let Some(old_value) = old_value_option
            && let Occupied(entry) = self.values_to_registries.entry(old_value) {
            entry.replace_entry(linked_registry.clone());
        };
        Ok(InsertionOk::Update)
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
