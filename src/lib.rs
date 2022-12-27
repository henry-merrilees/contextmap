#![feature(
    map_many_mut,
    entry_insert,
    is_some_and,
    let_chains,
    map_try_insert,
    specialization,
    type_changing_struct_update
)]
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::collections::{btree_map, hash_map};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::rc::Rc;

struct ContextRecord<C: Ord, L, V> {
    context: Rc<C>,
    link: Rc<L>,
    value: Option<Rc<V>>,
}

type ContextRegistry<C, L, V> = BTreeMap<Rc<C>, ContextRecord<C, L, V>>;

//  Rules:
//  - Records:
//    - A context, link and an optional value.
//  - Registries:
//    - An ordered map of records.
//    - The "value" of a record is the value of its most recent record (a None-valued record
//    means a none-valued registry)
//    - Write-only
//    - Records are inserted in cronological order
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
//  If we can gaurantee this rule, we can borrow mutably from either without worry that we have
//  already mutably borrowed the same RefCell from the other.
//
//
//
//  Guarantees:
//  - Registries:
//    - After a record has been inserted at a context, any query context not later than the last
//    inserted context will return the same value (bc write-only & chronological insertion rule).
//    - For space eficciency, we can regard an inserted record with the same value as the last
//    record as a no-op without erroring, (as query returns most recent, it does not change result
//    for any context)
//  - Update:
//    - the link_
//
pub struct ContextMap<L, C: Ord, V> {
    links_to_registries: HashMap<Rc<L>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
    values_to_registries: HashMap<Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
}

trait Command
where
    Self: Sized,
{
    fn execute(self) {}
}

/// Add new link with `Some<V>`.
struct NewLinkCommand<'a, C: Ord, L, V> {
    context: Rc<C>,
    link: Rc<L>,
    value: Rc<V>, // Enforcing value must be Some
    new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>,
    links_to_registries_entry:
        hash_map::VacantEntry<'a, Rc<L>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
    value_command: ValueCommand<'a, C, L, V>,
}

impl<'a, C: Ord, L, V> Command for NewLinkCommand<'a, C, L, V> {
    fn execute(self) {
        let NewLinkCommand {
            context,
            link,
            value,
            new_registry,
            links_to_registries_entry,
            value_command,
        } = self;

        let record: ContextRecord<C, L, V> = ContextRecord {
            context: context.clone(),
            link,
            value: Some(value),
        };

        {
            new_registry.get_mut().insert(context, record);
        }

        links_to_registries_entry.insert(new_registry);

        value_command.execute()
    }
}

/// Update a link pointing to a record with value `Some<V>` to a newer record with a different
/// value `Some<V>`.
struct UpdateCommand<'a, C: Ord, L, V> {
    link: Rc<L>,
    context: Rc<C>,
    value: Rc<V>, // Enforcing value must be Some
    context_registry: &'a mut ContextRegistry<C, L, V>,
    value_command: ValueCommand<'a, C, L, V>,
}

impl<'a, C: Ord, L, V> Command for UpdateCommand<'a, C, L, V> {
    fn execute(self) {
        let UpdateCommand {
            link,
            context,
            value,
            context_registry,
            value_command,
        } = self;

        let record = ContextRecord {
            link,
            context: context.clone(),
            value: Some(value),
        };

        context_registry.insert(context, record);

        value_command.execute();
    }
}

/// Command to update a link with a None value
struct NullifyCommand<'a, C: Ord, L, V> {
    link: Rc<L>,
    context: Rc<C>,
    values_to_registries_entry:
        hash_map::OccupiedEntry<'a, Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
}

impl<'a, C: Ord, L, V> NullifyCommand<'a, C, L, V> {
    fn execute(self) {
        let NullifyCommand {
            link,
            context,
            values_to_registries_entry,
        } = self;

        let record = ContextRecord {
            link,
            context: context.clone(),
            value: Option::<Rc<V>>::None,
        };

        values_to_registries_entry
            .get()
            .get_mut()
            .insert(context, record);
        values_to_registries_entry.remove();
    }
}

/// Command to fill an existing Record value None with Some value.
pub struct OverwriteCommand<'a, C: Ord, L, V> {
    value: Rc<V>,
    registry_entry: btree_map::OccupiedEntry<'a, Rc<C>, ContextRecord<C, L, V>>,
    value_command: ValueCommand<'a, C, L, V>,
}

impl<'a, C: Ord, L, V> Command for OverwriteCommand<'a, C, L, V> {
    fn execute(self) {
        let OverwriteCommand {
            value,
            mut registry_entry,
            value_command,
        } = self;

        registry_entry.get_mut().value = Some(value);

        value_command.execute();
    }
}

/// ## InsertionCommands are determined by the following:
///
/// Link does not exist: [`NewLink`](InsertionCommand::NewLink) (`Some(value)`, any context)
///
/// Link Exists:  
///
/// | ↓ value / context →         | Same Context                               | Later Context                          |
/// |-----------------------------|--------------------------------------------|----------------------------------------|
/// | `None` -> `Some(new_value)` | [`Overwrite`](InsertionCommand::Overwrite) | [`Update`](InsertionCommand::Update)   |
/// | `Some(old_value)` -> `None` | N/A (Destructive)                          | [`Nullify`](InsertionCommand::Nullify) |
///
///
/// All commands that insert `Some<V>`, i.e., [`NewLink`](InsertionCommand::NewLink),
/// [`Overwrite`](InsertionCommand::Overwrite), [`Update`](), may clash with existing links, which,
/// if existing, must be Nullified. As nullifying a ContextRegistry does not link `Some(V)`, it will
/// not clash. As such, every insertion operation should trigger at most two commands, the first to
/// associate the new link with the value, the second to nullify the old link.
enum InsertionCommand<'a, C: Ord, L, V> {
    NewLink(NewLinkCommand<'a, C, L, V>),
    Update(UpdateCommand<'a, C, L, V>),
    Nullify(NullifyCommand<'a, C, L, V>),
    Overwrite(OverwriteCommand<'a, C, L, V>),
    NoChange,
}

impl<'a, C: Ord, L, V> Command for InsertionCommand<'a, C, L, V> {
    fn execute(self) {
        match self {
            InsertionCommand::NewLink(command) => command.execute(),
            InsertionCommand::Update(command) => command.execute(),
            InsertionCommand::Nullify(command) => command.execute(),
            InsertionCommand::Overwrite(command) => command.execute(),
            InsertionCommand::NoChange => {}
        }
    }
}

enum ValueCommand<'a, C: Ord, L, V> {
    Vacant {
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>,
        vacant_values_to_registries_entry:
            hash_map::VacantEntry<'a, Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
    },
    Occupied {
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>,
        vacant_values_to_registries_entry:
            hash_map::VacantEntry<'a, Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>,
        nullify_command: NullifyCommand<'a, C, L, V>,
    },
}


impl<'a, C: Ord, L, V> Command for ValueCommand<'a, C, L, V> {
    fn execute(self) {
        match self {
            ValueCommand::Vacant {
                new_registry,
                vacant_values_to_registries_entry,
            } => {
                vacant_values_to_registries_entry.insert(new_registry);
            }
            ValueCommand::Occupied {
                new_registry,
                nullify_command,
            } => {
                nullify_command.execute();
            }
        }
    }
}

#[derive(Debug)]
enum RecordToEntryError {
    OutdatedContext,
    OverwritingSome,
}

impl Display for RecordToEntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for RecordToEntryError {}

impl<L, C, V> ContextMap<L, C, V>
where
    L: PartialEq + Eq + Hash,
    C: Ord + Debug,
    V: Hash + Eq + Debug,
{
    fn new() -> Self {
        Self {
            links_to_registries: HashMap::<Rc<L>, Rc<RefCell<ContextRegistry<C, L, V>>>>::new(),
            values_to_registries: HashMap::<Rc<V>, Rc<RefCell<ContextRegistry<C, L, V>>>>::new(),
        }
    }

    fn gen_value_command<'a>(
        &mut self,
        link: Rc<L>,
        context: Rc<C>,
        new_value: Rc<V>,
        new_registry: Rc<RefCell<ContextRegistry<C, L, V>>>, 
    ) -> ValueCommand<'a, C, L, V> {
        let entry = self.values_to_registries.entry(new_value);
        match entry {
            hash_map::Entry::Occupied(occupied_entry) => ValueCommand::Occupied {
                new_registry,
                nullify_command: NullifyCommand {
                    link,
                    context,
                    values_to_registries_entry: occupied_entry,
                },
            },
            hash_map::Entry::Vacant(vacant_entry) => ValueCommand::Vacant {
                vacant_values_to_registries_entry: vacant_entry,
            },
        }
    }

    fn generate_command(
        &mut self,
        context: Rc<C>,
        link: Rc<L>,
        value: Rc<V>,
    ) -> Result<InsertionCommand<C, L, V>, RecordToEntryError> {
        // So long as we can assure that we never pass the same
        let linked_registry_entry = self.links_to_registries.entry(link);
        let valued_registry_entry = self.values_to_registries.entry(value);

        match (linked_registry_entry, valued_registry_entry) {
            (
                hash_map::Entry::Occupied(linked_registry_occupied),
                hash_map::Entry::Occupied(valued_registry_occupied),
            ) => {
                if std::ptr::eq(
                    linked_registry_occupied.get(),
                    valued_registry_occupied.get(),
                ) {
                    // The link and value already point to the same registry, they are already associated.
                    return Ok(InsertionCommand::NoChange);
                } else {
                    let value_command = todo!();
                    let update_command = UpdateCommand {
                        link,
                        context,
                        value,
                        value_command,
                        context_registry: linked_registry_occupied.get().get_mut(),
                    };
                    Ok(InsertionCommand::Update(update_command));
                }
            }
            (
                hash_map::Entry::Occupied(occupied_link_registry_entry),
                hash_map::Entry::Occupied(occupied_value_registry_entry),
            ) => {}
            (hash_map::Entry::Occupied(_), hash_map::Entry::Vacant(_)) => {
                // link registry => Ok: update, overwrite || Err: overwritesome, or context
            }
            (hash_map::Entry::Vacant(_), hash_map::Entry::Occupied(_)) => todo!(),
            (hash_map::Entry::Vacant(_), hash_map::Entry::Vacant(_)) => todo!(),
        }

        // is there an existing link (if not, NewLink, otherwise, can we overwrite / update?)
        // does the value exist in another link

        // if let hash_map::Entry::Occupied(occupied_value_registry_entry) = value_registry_entry
        // && let  Some((old_value_context, _)) = occupied_value_registry_entry.get().borrow().last_key_value()
        // && &context < old_value_context {
        // return Err(RecordToEntryError::OutdatedContext);
        // };
    }

    fn insert_record(&mut self, link: L, record: ContextRecord<C, L, V>)
    /*  -> Result<(), Box<dyn Error>> */
    {
        let link_registry = self.links_to_registries.get(&link);
        let value_registry = record.value.and_then(|v| self.values_to_registries.get(&v));

        match (link_registry, value_registry) {
            //Need to create new registry.
            (None, None) => link_registry = todo!(),
            (None, Some(value_registry)) => todo!(),
            (None, Some(value_registry)) => todo!(),
            (Some(_), None) => todo!(),
            (Some(_), Some(_)) => todo!(),
        }
    }
}
