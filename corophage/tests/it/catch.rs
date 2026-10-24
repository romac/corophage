use std::future::Future;

use corophage::prelude::*;

#[derive(Debug, PartialEq)]
struct NotFound(u64);

struct Lookup(u64);

impl Effect for Lookup {
    type Resume<'r> = Option<&'static str>;

    fn shorten_resume<'a: 'b, 'b>(resume: Option<&'static str>) -> Option<&'static str> {
        resume
    }
}

struct Stop;

impl Effect for Stop {
    type Resume<'r> = Never;

    fn shorten_resume<'a: 'b, 'b>(resume: Never) -> Never {
        resume
    }
}

type Effs = Effects![Lookup, Stop, Throw<NotFound>];

fn find(id: u64) -> Program<'static, Effs, impl Future<Output = usize>> {
    Program::new(move |mut y: Yielder<'_, Effs>| async move {
        if id == 0 {
            match y.yield_(Stop).await {}
        }
        let name = match y.yield_(Lookup(id)).await {
            Some(name) => name,
            None => y.throw(NotFound(id)).await,
        };
        name.len()
    })
}

fn lookup(Lookup(id): Lookup) -> Control<Option<&'static str>> {
    Control::resume((id == 1).then_some("alice"))
}

#[test]
fn completion_throw_and_cancellation_stay_distinct() {
    let run = |id| {
        find(id)
            .handle(lookup)
            .handle(|_: Stop| Control::cancel())
            .catch()
            .run_sync()
    };

    assert_eq!(run(1), Ok(Ok(5)));
    assert_eq!(run(2), Ok(Err(NotFound(2))));
    assert_eq!(run(0), Err(Cancelled));
}

#[test]
fn question_mark_propagates_cancellation_and_keeps_the_error() {
    fn name_length(id: u64) -> Result<Result<usize, NotFound>, Cancelled> {
        let result = find(id)
            .handle(lookup)
            .handle(|_: Stop| Control::cancel())
            .catch()
            .run_sync()?;
        Ok(result)
    }

    assert_eq!(name_length(2), Ok(Err(NotFound(2))));
    assert_eq!(name_length(0), Err(Cancelled));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_stateful_catch_is_send() {
    fn assert_send<T: Send>(value: T) -> T {
        value
    }

    let mut lookups = 0;
    let future = assert_send(
        find(2)
            .with_state(&mut lookups)
            .handle(async |lookups: &mut usize, effect: Lookup| {
                *lookups += 1;
                tokio::task::yield_now().await;
                lookup(effect)
            })
            .handle(|_: Stop| Control::cancel())
            .catch()
            .run(),
    );

    assert_eq!(future.await, Ok(Err(NotFound(2))));
    assert_eq!(lookups, 1);
}

#[test]
fn throw_from_an_invoked_child_skips_the_rest_of_the_parent() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REACHED_AFTER_INVOKE: AtomicBool = AtomicBool::new(false);

    let parent = Program::new(|mut y: Yielder<'_, Effs>| async move {
        let length = y.invoke(find(2)).await;
        REACHED_AFTER_INVOKE.store(true, Ordering::Relaxed);
        length
    });

    let result = parent
        .handle(lookup)
        .handle(|_: Stop| Control::cancel())
        .catch()
        .run_sync();

    assert_eq!(result, Ok(Err(NotFound(2))));
    assert!(!REACHED_AFTER_INVOKE.load(Ordering::Relaxed));
}

#[test]
fn throw_in_statement_position_and_ordinary_handling() {
    let program = || {
        Program::new(|mut y: Yielder<'_, Effects![Throw<NotFound>]>| async move {
            if true {
                return y.throw(NotFound(7)).await;
            }
            "unreachable"
        })
    };

    assert_eq!(program().catch().run_sync(), Ok(Err(NotFound(7))));

    // Without `catch`, `Throw` is an ordinary effect.
    let mut seen = None;
    let result = program()
        .handle(|Throw(error): Throw<NotFound>| {
            seen = Some(error);
            Control::cancel()
        })
        .run_sync();
    assert_eq!(result, Err(Cancelled));
    assert_eq!(seen, Some(NotFound(7)));
}

#[test]
fn handlers_state_and_mapping_can_follow_catch() {
    let mut lookups = 0;
    let result = find(2)
        .catch()
        .with_state(&mut lookups)
        .handle(|lookups: &mut usize, effect: Lookup| {
            *lookups += 1;
            lookup(effect)
        })
        .handle(|_: Stop| Control::cancel())
        .map(|caught| caught.map_err(|NotFound(id)| id))
        .run_sync();

    assert_eq!(result, Ok(Err(2)));
    assert_eq!(lookups, 1);
}

#[test]
fn a_caught_throw_drops_the_rest_of_the_body() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static DROPPED: AtomicUsize = AtomicUsize::new(0);
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }

    let result = Program::new(|mut y: Yielder<'_, Effs>| async move {
        let _guard = Guard;
        let found = y.yield_(Lookup(2)).await;
        if found.is_none() {
            // The body is dropped here; the handler for `Throw` never runs.
            y.throw::<(), _, _>(NotFound(2)).await;
        }
        unreachable!("the throw completes the program")
    })
    .handle(lookup)
    .handle(|_: Stop| Control::cancel())
    .catch()
    .map(|caught: Result<(), NotFound>| {
        assert_eq!(DROPPED.load(Ordering::Relaxed), 1);
        caught
    })
    .run_sync();

    assert_eq!(result, Ok(Err(NotFound(2))));
}

#[derive(Debug, PartialEq)]
struct Invalid;

#[test]
fn nested_catches_take_their_own_errors() {
    type Both = Effects![Throw<NotFound>, Throw<Invalid>];
    let program = |which: u8| {
        Program::new(move |mut y: Yielder<'_, Both>| async move {
            match which {
                0 => y.throw(NotFound(1)).await,
                1 => y.throw(Invalid).await,
                _ => 7,
            }
        })
        .catch::<NotFound, _, _, _>()
        .catch::<Invalid, _, _, _>()
        .run_sync()
    };

    assert_eq!(program(0), Ok(Ok(Err(NotFound(1)))));
    assert_eq!(program(1), Ok(Err(Invalid)));
    assert_eq!(program(2), Ok(Ok(Ok(7))));
}

#[test]
fn a_withdrawn_throw_is_not_caught() {
    let result = Program::new(|mut y: Yielder<'_, Effects![Throw<NotFound>]>| async move {
        tokio::select! {
            biased;
            never = y.throw::<Never, _, _>(NotFound(3)) => match never {},
            () = std::future::ready(()) => "recovered",
        }
    })
    .catch()
    .run_sync();

    assert_eq!(result, Ok(Ok("recovered")));
}
