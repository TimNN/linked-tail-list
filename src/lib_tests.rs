use std::fmt::Debug;
use test;
use super::{swap_places, TailList, Link, LinkOwn, OwnRef};

/// List validation utility, see method documentation
struct Validator<T>(*const LinkOwn<T>);

impl<T: Debug> Validator<T> {
    /// Create a new `Validator`.
    ///
    /// The list which should be validated **MUST NOT BE MOVED**.
    fn new(list: &TailList<T>) -> Validator<T> {
        Validator(&list.head)
    }

    /// Validate the list, `line` is used in the error message.
    fn validate(&self, line: u32) {
        unsafe {
            let mut this_link = self.0;
            let mut next_ref_opt = (*this_link).borrow_inner().opt_node_ref();

            while let Some(next_ref) = next_ref_opt {
                let l_ptr: *mut Link<T> = (*this_link).0.get();
                let ol_ptr: *mut Link<T> = next_ref.borrow_inner().owning_link.0;

                assert!(l_ptr == ol_ptr, "invalid list ptr at line {}", line);

                this_link = &next_ref.borrow_inner().next;
                next_ref_opt = (*this_link).borrow_inner().opt_node_ref();
            }
        }
    }

    /// Dumps the complete list of this validator to stdout.
    ///
    /// This function may be used when debugging failing tests.
    #[allow(dead_code)]
    fn dump(&self) {
        println!("\n== BEGINNING VALIDATION ==");
        Validator::dump_tail(self.0);
    }

    /// Dump a node and its tail recursivly.
    fn dump_tail(this_link: *const LinkOwn<T>) {
        unsafe {
            let next_ref_opt = (*this_link).borrow_inner().opt_node_ref();
            println!("OwnLink @ {:?}", this_link);

            if let Some(next_ref) = next_ref_opt {
                println!("Linking to OwnNode @ {:?}", next_ref.get_mut_ptr());
                println!("With value: {:?}", next_ref.borrow_inner().val);
                println!("With owning_link to {:?}", next_ref.borrow_inner()
                    .owning_link.get_mut_ptr());
                println!("");
                Validator::dump_tail(&next_ref.borrow_inner().next);
            }
        }
    }
}

/// Call `$v.validate()` with the current line number iff compiling without the
/// `test_no_validate` feature.
macro_rules! validate {
    ($v:ident) => (if cfg!(feature="test_no_validate") {} else { $v.validate(line!()) })
}

#[test]
fn it_swaps_places() {
    println!("");

    let mut list = TailList::new();
    let v = Validator::new(&list);

    list.push(0);
    list.push(1);
    validate!(v);

    let a = list.head.borrow_inner().opt_node_ref().unwrap();
    let b = a.borrow_inner().next.borrow_inner().opt_node_ref().unwrap();

    validate!(v);

    swap_places(&a, &b);

    validate!(v);
}

#[test]
fn fill_and_drop() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    validate!(v);

    for i in 0u64..2048 {
        list.push(i);
        validate!(v);
    }

    drop(test::black_box(list));
}

#[test]
fn fill_and_iter() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    validate!(v);

    for i in 0u64..2048 {
        list.push(i);
        validate!(v);
    }

    let mut cursor = list.cursor();
    validate!(v);

    for i in 0u64..2048 {
        let i = 2047 - i;

        assert_eq!(cursor.next().map(|i| *i), Some(i));
        validate!(v);
    }

    assert!(cursor.next().is_none());
    validate!(v);
}

#[test]
fn iter_insert() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    for i in 0u64..1024 {
        list.push(i);
    }

    validate!(v);

    {
        let mut cursor = list.cursor();

        for i in 0u64..1024 {
            let i = 1023 - i;

            cursor.next().unwrap().insert_before(i);
            validate!(v);
        }

        assert!(cursor.next().is_none());
    }

    let mut cursor = list.cursor();

    for i in 0u64..1024 {
        let i = 1023 - i;

        assert_eq!(cursor.next().map(|i| *i), Some(i));
        assert_eq!(cursor.next().map(|i| *i), Some(i));
    }

    assert!(cursor.next().is_none());
    validate!(v);
}

#[test]
fn iter_tail() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    for i in 0u64..512 {
        list.push(i);
    }
    validate!(v);

    let mut cursor = list.cursor();

    for i in 0u64..512 {
        let mut next = cursor.next().unwrap();
        let (next, mut tail) = next.tail();
        validate!(v);

        assert_eq!(**next, 511 - i);

        for j in i + 1 .. 512 {
            assert_eq!(tail.next().map(|j| *j), Some(511 - j));
            validate!(v);
        }

        assert!(tail.next().is_none());

        assert_eq!(**next, 511 - i);
        validate!(v);
    }
}

#[test]
fn iter_into_tail() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    for i in 0u64..512 {
        list.push(i);
    }
    validate!(v);

    let mut cursor = list.cursor();

    for i in 0u64..512 {
        let next = cursor.next().unwrap();
        let (next, mut tail) = next.into_tail();
        validate!(v);

        assert_eq!(*next, 511 - i);

        for j in i + 1 .. 512 {
            assert_eq!(tail.next().map(|j| *j), Some(511 - j));
            validate!(v);
        }

        assert!(tail.next().is_none());

        assert_eq!(*next, 511 - i);
        validate!(v);
    }
}

#[test]
fn remove_all_iter() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    for i in 0u64..1024 {
        list.push(i);
    }

    {
        let mut cursor = list.cursor();

        for i in 0u64..1024 {
            let i = 1023 - i;

            assert_eq!(cursor.next().unwrap().remove(), i);
            validate!(v);
        }

        assert!(cursor.next().is_none());
    }

    let mut cursor = list.cursor();
    assert!(cursor.next().is_none());
    validate!(v);
}

#[test]
fn mark_all() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    {
        let mut vec = Vec::with_capacity(1024);

        for i in 0u64..1024 {
            list.push(i);
        }

        {
            let mut cursor = list.cursor();

            for _ in 0u64..1024 {
                vec.push(cursor.next().unwrap().into_passive());
                validate!(v);
            }

            assert!(cursor.next().is_none());
        }

        for i in 0u64..1024 {
            assert_eq!(vec.remove((1023 - i) as usize).remove(), i);
            validate!(v);
        }
    }

    let mut cursor = list.cursor();
    assert!(cursor.next().is_none());
}

#[test]
fn remove_mark_alternate() {
    let mut list = TailList::new();
    let v = Validator::new(&list);
    let mut vec = Vec::with_capacity(512);

    for i in 0u64..1024 {
        list.push(i);
    }

    {
        let mut cursor = list.cursor();

        for i in 0u64..1024 {
            let i = 1023 - i;

            if i % 2 == 0 {
                vec.push(cursor.next().unwrap().into_passive());
            } else {
                assert_eq!(cursor.next().unwrap().remove(), i);
            }
            validate!(v);
        }
    }

    for i in 0u64..512 {
        assert_eq!(vec.remove((511 - i) as usize).remove(), 2 * i);
        validate!(v);
    }
}

#[test]
fn remove_tail() {
    let mut list = TailList::new();
    let v = Validator::new(&list);

    list.push(0);
    list.push(1);

    let mut cursor = list.cursor();

    {
        let mut next = cursor.next().unwrap();
        let (_, mut tail) = next.tail();

        assert!(tail.next().unwrap().remove() == 0);
        validate!(v);
    }

    assert!(cursor.next().is_none());
    validate!(v);
}
