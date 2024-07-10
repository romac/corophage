use std::future::Future;
use std::pin::Pin;

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, ScopedCoroutine};

pub mod handler;

#[derive(Debug)]
pub enum Msg {
    StartHeight(u64),
    GossipEvent(String),
}

#[derive(Debug)]
pub enum Yield {
    GetValue(u64),
}

#[derive(Debug)]
pub enum Resume {
    Start,
    ProposeValue(char),
}

pub type Co<'a> = ScopedCoroutine<'a, Resume, Yield, Result<(), BoxError>, DefaultStack>;
pub type CoResult = CoroutineResult<Yield, Result<(), BoxError>>;
pub type BoxError = Box<dyn std::error::Error>;

#[macro_export]
macro_rules! expect_resume {
    ($result:expr) => {
        expect_resume!($result, $crate::Resume::Continue)
    };

    ($result:expr, $pat:pat) => {
        expect_resume!($result, $pat => ())
    };

    ($result:expr $(, $pat:pat => $expr:expr)+ $(,)*) => {
        match $result {
            $($pat => $expr,)+
            resume => {
                return Err(format!(
                    "Unexpected resume: {resume:?}, expected one of: {}",
                    concat!(concat!($(stringify!($pat))+), ", ")
                )
                .into())
            }
        }
    };
}

fn co<'a>(msg: Msg) -> Co<'a> {
    Coroutine::new(move |yielder, start| {
        assert!(matches!(start, Resume::Start));

        println!("Started with msg: {msg:?}");

        match msg {
            Msg::StartHeight(height) => {
                let resume = yielder.suspend(Yield::GetValue(height));
                let value: char = expect_resume!(resume,
                    Resume::ProposeValue(value) => value
                );

                println!("Proposed value: {value}");
            }
            Msg::GossipEvent(event) => {
                println!("Gossip event: {event}");
            }
        }

        Ok(())
    })
}

pub fn handle_sync<'a>(
    msg: Msg,
    mut on_yield: impl FnMut(Yield) -> Resume + 'a,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut co = co(msg);
    let mut yielded = co.resume(Resume::Start);

    loop {
        match yielded {
            CoResult::Yield(yld) => yielded = co.resume(on_yield(yld)),
            CoResult::Return(result) => return result,
        }
    }
}

pub async fn handle_async<'a>(
    msg: Msg,
    mut on_yield: impl FnMut(Yield) -> Pin<Box<dyn Future<Output = Resume> + Send + 'a>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut co = co(msg);
    let mut yielded = co.resume(Resume::Start);

    loop {
        match yielded {
            CoResult::Yield(yld) => yielded = co.resume(on_yield(yld).await),
            CoResult::Return(result) => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_msg_ok() {
        fn process_msg(msg: Msg) -> Result<(), Box<dyn std::error::Error>> {
            handle_sync(msg, |yielded| match yielded {
                Yield::GetValue(height) => {
                    println!("Yielded GetValue({height})");
                    Resume::ProposeValue('A')
                }
            })
        }

        process_msg(Msg::StartHeight(1)).unwrap();
        process_msg(Msg::GossipEvent("Hello".to_string())).unwrap();
    }

    // #[tokio::test]
    // async fn process_msg_ok_async() {
    //     struct State {
    //         value: char,
    //     }
    //
    //     async fn get_value(state: &mut State) -> char {
    //         let value = state.value;
    //         state.value = (state.value as u8 + 1) as char;
    //         value
    //     }
    //
    //     async fn process_msg(msg: Msg) -> Result<(), Box<dyn std::error::Error>> {
    //         let mut state = State { value: 'A' };
    //
    //         handle_async(msg, |yielded| {
    //             Box::pin(async {
    //                 match yielded {
    //                     Yield::GetValue(height) => {
    //                         println!("Yielded GetValue({height})");
    //                         let value = get_value(&mut state).await;
    //                         Resume::ProposeValue(value)
    //                     }
    //                 }
    //             })
    //         })
    //         .await
    //     }
    //
    //     process_msg(Msg::StartHeight(1)).await.unwrap();
    //     process_msg(Msg::GossipEvent("Hello".to_string()))
    //         .await
    //         .unwrap();
    // }

    #[test]
    #[should_panic = "Unexpected resume: Start, expected one of: Resume::ProposeValue"]
    fn wrong_resume() {
        fn process_msg(msg: Msg) -> Result<(), Box<dyn std::error::Error>> {
            handle_sync(msg, |yielded| match yielded {
                Yield::GetValue(height) => {
                    println!("Yielded GetValue({height})");
                    Resume::Start
                }
            })
        }

        process_msg(Msg::StartHeight(1)).unwrap();
    }
}
