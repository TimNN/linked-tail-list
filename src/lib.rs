//! This module implements a specialized linked list.
//!
//! The implemented list (from now on called a tail list) is neither a singly
//! nor doubly linked: aside from the optional link to the next node, each node
//! also has reference to the link which owns the node.
//!
//! For each tail list there is one item which currently 'owns' a node and it's
//! tail (all the following nodes). 'Owns' in this context does not mean actual
//! ownership in rust terms, but rather 'mutably borrows' this node (and it's)
//! tail. This item will be referred to as the *active* item.
//!
//! For each node which is not owned by the currently active item, there exists
//! at most one *passive* item, which owns a single node, but neither it's
//! predecessors nor successors.
//!
//! A `TailList`, `Cursor` and `TailValRef` are active items. A `ValRef` is a
//! passive item.
//!
//! An active item may temporarily transfer ownership of it's owned node to
//! another item by creating a mutable borrow to itself.

#![cfg_attr(test, feature(test))]

#[cfg(test)] extern crate test;

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};

////////////////////////////////////////////////////////////////////////////////
// STRUCTS
////////////////////////////////////////////////////////////////////////////////

/// A struct actually owning its contents.
struct Own<T>(UnsafeCell<T>); // TODO?: NonZero

/// A struct only referencing its contents.
struct Ref<T>(*mut T); // TODO: NonZero

/// An actual Link to another Node.
struct Link<T>(Option<Box<NodeOwn<T>>>);

/// A Link which actually owns it's contents.
type LinkOwn<T> = Own<Link<T>>;

/// A reference to a Link.
type LinkRef<T> = Ref<Link<T>>;

/// An actual Node. Iff `val` is `None`, this is a dummy Node.
struct Node<T> {
    next: LinkOwn<T>,
    owning_link: LinkRef<T>,
    val: Option<T>,
}

/// A Node which actually owns it's contents.
type NodeOwn<T> = Own<Node<T>>;

/// A reference to a Node.
type NodeRef<T> = Ref<Node<T>>;

/// A specialized linked list (see the module documentation).
pub struct TailList<T> {
    head: LinkOwn<T>,
}

// Lifetimes:
// (Only the listed lifetimes may be used and only for their intended meaning)
//
// 'node: The lifetime of any single node, usually the lifetime of the list
// 'tail: The lifetime of the tail (only used for TailValRef)
// 'slf:  Used in function calls to explicitly denote the self lifetime

/// A `Cursor` is an iterator over a node and it's tail. It is an active item,
/// which owns the next node it would return.
///
/// Due to the design of rust's `Iterator` trait, `Cursor` cannot implement
/// `Iterator`.
pub struct Cursor<'node, T: 'node> {
    dummy: NodeRef<T>,
    phantom: PhantomData<&'node mut Node<T>>,
}

/// A `ValRef` is a passive item, which provides mutable access to a single
/// node.
pub struct ValRef<'node, T: 'node> {
    node: NodeRef<T>,
    phantom: PhantomData<&'node Node<T>>,
}

/// A `TailValRef` is an active item, which provides mutable access to a single
/// node and its successors.
pub struct TailValRef<'node, 'tail, T: 'node + 'tail> {
    val_ref: ValRef<'node, T>,
    phantom: PhantomData<Cursor<'tail, T>>,
}

////////////////////////////////////////////////////////////////////////////////
// IMPLS
////////////////////////////////////////////////////////////////////////////////

impl<T> Own<T> {
    /// Returns a new `Own` struct encapsulating the given value.
    fn new(val: T) -> Own<T> { Own(UnsafeCell::new(val)) }
}

impl<T> Link<T> {
    /// Returns a new `Link` linking to nothing.
    fn new() -> Link<T> { Link(None) }

    /// Returns as new `Link` linking to the given node.
    fn new_to_node(node: NodeOwn<T>) -> Link<T> {
        Link(Some(Box::new(node)))
    }

    /// Returns an optional `NodeRef` to the linked to node, any.
    fn opt_node_ref(&self) -> Option<NodeRef<T>> {
        self.0.as_ref().map(|owned| owned.new_ref())
    }
}

impl<T> Node<T> {
    /// Returns a new node with the given `val` and `owning_link`.
    fn new(val: Option<T>, owning_link: LinkRef<T>) -> Node<T> {
        Node {
            next: Own::new(Link::new()),
            owning_link: owning_link,
            val: val,
        }
    }
}

impl<T> TailList<T> {
    /// Creates a new empty list.
    pub fn new() -> TailList<T> {
        TailList {
            head: Own::new(Link::new()),
        }
    }

    /// Pushed a new element to the front of the list.
    pub fn push(&mut self, val: T) {
        insert_at(&self.head, Some(val));
    }

    /// Returns a cursor over all elements in this list.
    pub fn cursor<'node>(&'node mut self) -> Cursor<'node, T> {
        Cursor::new(&self.head.new_ref())
    }
}

impl<'node, T: 'node> Cursor<'node, T> {
    /// Returns a new cursor with it's dummy node inserted after the given link.
    fn new(at: &LinkRef<T>) -> Cursor<'node, T> {
        Cursor {
            dummy: insert_at(at, None),
            phantom: PhantomData,
        }
    }

    /// (Optionally) returns the next element of this cursor.
    ///
    /// This cursor is unusable as long as the `'tail` lifetime is still
    /// referenced.
    pub fn next<'tail>(&'tail mut self) -> Option<TailValRef<'node, 'tail, T>> {
        // Get a reference to the next node, if there is any
        let next_ref_opt: Option<NodeRef<T>> = self.dummy.borrow_inner().next
            .borrow_inner().opt_node_ref();

        let next_ref = match next_ref_opt {
            Some(next_ref) => next_ref,
            None => return None,
        };

        // Swap the places of the dummy and next nodes in the list
        swap_places(&self.dummy, &next_ref);

        // If the next node happens to be a dummy node, skip it by calling next
        // again
        if next_ref.borrow_inner().val.is_none() {
            return self.next();
        }

        // Return the next node
        Some(TailValRef {
            val_ref: ValRef {
                node: next_ref,
                phantom: PhantomData,
            },
            phantom: PhantomData,
        })
    }
}

impl<'node, T: 'node> ValRef<'node, T> {
    /// Returns a new ValRef referencing the given node.
    fn new(node: NodeRef<T>) -> ValRef<'node, T> {
        ValRef {
            node: node,
            phantom: PhantomData,
        }
    }

    /// Inserts a new element before this element and returns a `ValRef` to the
    /// newly inserted element.
    pub fn insert_before(&mut self, val: T) -> ValRef<'node, T> {
        ValRef::new(insert_at(&self.node.borrow_inner().owning_link, Some(val)))
    }

    /// Inserts a new element after this element and returns a `ValRef` to the
    /// newly inserted element.
    pub fn insert_after(&mut self, val: T) -> ValRef<'node, T> {
        ValRef::new(insert_at(&self.node.borrow_inner().next, Some(val)))
    }

    /// Removes this element from the list and returns it's value.
    pub fn remove(self) -> T {
        let val = unlink(self.node);

        if let Some(val) = val {
            return val;
        }

        unreachable!("cannot remove dummy node")
    }
}

impl<'node, 'tail, T: 'node + 'tail> TailValRef<'node, 'tail, T> {
    /// Returns a reference to a `ValRef` to the first node owned by `self`, as
    /// well as a `Cursor` owning the rest of the nodes owned by `self`.
    ///
    /// After both items have gone out of scope, this method may be called
    /// again.
    pub fn tail<'slf>(&'slf mut self) -> (&'slf ValRef<'node, T>,
                                          Cursor<'slf, T>) {
        let csr = Cursor::new(&self.val_ref.node.borrow_inner().next.new_ref());
        (&self.val_ref, csr)
    }

    /// Returns a `ValRef` to the first node owned by `self, as well as a
    /// `Cursor` owning the rest of the nodes owned by `self`.
    ///
    /// This method consumes `self`. The `Cursor` who returned this may be used
    /// again after the returned cursor has gone out of scope.
    pub fn into_tail(self) -> (ValRef<'node, T>, Cursor<'tail, T>) {
        let csr = Cursor::new(&self.val_ref.node.borrow_inner().next.new_ref());
        (self.val_ref, csr)
    }

    /// Turns `self` into a `ValRef` to the first node owned by `self`. The
    /// `Cursor` who returned this may be used again after this method has been
    /// called.
    pub fn into_passive(self) -> ValRef<'node, T> {
        self.val_ref
    }

    /// Inserts a new element before this element and returns a `ValRef` to the
    /// newly inserted element.
    pub fn insert_before(&mut self, val: T) -> ValRef<'node, T> {
        self.val_ref.insert_before(val)
    }

    /// Inserts a new element after this element and returns a `ValRef` to the
    /// newly inserted element.
    pub fn insert_after(&mut self, val: T) -> ValRef<'node, T> {
        self.val_ref.insert_after(val)
    }

    /// Removes this element from the list and returns it's value.
    pub fn remove(self) -> T {
        self.val_ref.remove()
    }
}

////////////////////////////////////////////////////////////////////////////////
// FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

/// Inserts a new node into the list, directly at / after `link`.
fn insert_at<T, L: OwnRef<Inner=Link<T>>>(link: &L, val: Option<T>) -> NodeRef<T> {
    // Create a `NodeOwn` for the new value
    let node = Own::new(Node::new(val, link.new_ref()));

    // Move the tail of `link` to `node.next`
    let link: &mut Link<T> = link.borrow_inner_mut();

    {
        let next: &mut Link<T> = node.borrow_inner().next.borrow_inner_mut();

        mem::swap(link, next);
    }

    // Make `link` link to the new node
    *link = Link::new_to_node(node);

    // Get a reference to the newly created node
    let node_ref = link.opt_node_ref()
        .expect("the option was just initialized to some");

    // Fix the `owning_link` of node originally linked to by `link` (which is
    // now linked to by node.next)
    fixup_owning_link(&node_ref.borrow_inner().next);

    // Return a reference to the newly created node
    node_ref
}

/// Swap the places of two nodes in the list
fn swap_places<T>(a: &NodeRef<T>, b: &NodeRef<T>) {
    // Swap the actual nodes (in the owning links)
    let a_link = a.borrow_inner().owning_link.borrow_inner_mut();
    let b_link = b.borrow_inner().owning_link.borrow_inner_mut();

    mem::swap(a_link, b_link);

    // Swap the next links
    let a_next = a.borrow_inner().next.borrow_inner_mut();
    let b_next = b.borrow_inner().next.borrow_inner_mut();

    mem::swap(a_next, b_next);

    // Fix up all owning links
    fixup_owning_link(&a.borrow_inner().owning_link);
    fixup_owning_link(&b.borrow_inner().owning_link);
    fixup_owning_link(&a.borrow_inner().next);
    fixup_owning_link(&b.borrow_inner().next);
}

/// Unlinks / removes the given node from the list and returns its optional
/// value.
fn unlink<T>(node_ref: NodeRef<T>) -> Option<T> {
    // A mutable borrow of the next link of the node to remove
    let next: &mut Link<T> = node_ref.borrow_inner().next
        .borrow_inner_mut();

    // A reference to the link owning the node to remove
    let owning_link_ref: LinkRef<T> = node_ref.borrow_inner().owning_link
        .clone();

    // A mutable borrow of the link linking to the node to remove
    let owning_link: &mut Link<T> = owning_link_ref.borrow_inner_mut();

    // A temporary owning link
    let tmp_link_own = Own::new(Link::new());

    // Remove the node from the list
    {
        // A mutable borrow of the link owned by `tmp_link_own`
        let tmp_link: &mut Link<T> = tmp_link_own.borrow_inner_mut();

        // tmp -> None, owning -> This, next -> Next
        mem::swap(tmp_link, owning_link);
        // tmp -> This, owning -> None, next -> Next
        mem::swap(owning_link, next);
        // tmp -> This, owning -> Next, next -> None

        fixup_owning_link(&owning_link_ref);
    }

    // Extract the value now owned by `tmp_link_own`
    unsafe {
        let val: NodeOwn<T> = *tmp_link_own.0.into_inner().0
            .expect("the option was just set to some");

        val.0.into_inner().val
    }
}

/// Given a link, if this link links to a node, ensures that the node's
/// `owning_link` points to the given link.
fn fixup_owning_link<T, L: OwnRef<Inner=Link<T>>>(link: &L) {
    let opt_node_ref = link.borrow_inner().opt_node_ref();

    if let Some(node_ref) = opt_node_ref {
        let node = node_ref.borrow_inner_mut();
        node.owning_link = link.new_ref();
    }
}

////////////////////////////////////////////////////////////////////////////////
// TRAITS
////////////////////////////////////////////////////////////////////////////////

/// A trait abstracting over `Own` and `Ref`.
trait OwnRef {
    /// The type encapsulated by this `Own` or `Ref`.
    type Inner;

    /// Returns a mutable pointer to the inner value.
    fn get_mut_ptr(&self) -> *mut Self::Inner;

    /// Returns a new `Ref` to the inner value.
    fn new_ref(&self) -> Ref<Self::Inner> {
        Ref(self.get_mut_ptr())
    }

    /// Borrows the inner value.
    fn borrow_inner(&self) -> &Self::Inner {
        unsafe { & *self.get_mut_ptr() }
    }

    /// Borrows the inner value mutably.
    fn borrow_inner_mut(&self) -> &mut Self::Inner {
        unsafe { &mut *self.get_mut_ptr() }
    }
}

////////////////////////////////////////////////////////////////////////////////
// TRAIT IMPLS
////////////////////////////////////////////////////////////////////////////////

impl<T> OwnRef for Own<T> {
    type Inner = T;

    fn get_mut_ptr(&self) -> *mut T { self.0.get() }
}

impl<T> OwnRef for Ref<T> {
    type Inner = T;

    fn get_mut_ptr(&self) -> *mut T { self.0 }
}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Ref<T> {
        Ref(self.0)
    }
}

impl <'node, T: 'node> Drop for Cursor<'node, T> {
    fn drop(&mut self) {
        unlink(self.dummy.clone());
    }
}

impl<'node, T: 'node> Deref for ValRef<'node, T> {
    type Target = T;

    fn deref(&self) -> &T {
        if let Some(ref val) = self.node.borrow_inner().val {
            return val;
        }

        unreachable!("cannot deref dummy node");
    }
}

impl<'node, T: 'node> DerefMut for ValRef<'node, T> {
    fn deref_mut(&mut self) -> &mut T {
        if let Some(ref mut val) = self.node.borrow_inner_mut().val {
            return val;
        }

        unreachable!("cannot deref dummy node");
    }
}

impl<'node, 'tail, T: 'node + 'tail> Deref for TailValRef<'node, 'tail, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.val_ref.deref()
    }
}

impl<'node, 'tail, T: 'node + 'tail> DerefMut for TailValRef<'node, 'tail, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.val_ref.deref_mut()
    }
}

#[cfg(test)]
mod tests {
    include!( "./lib_tests.rs");
}
