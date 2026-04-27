use core::future::Future;

#[doc(hidden)]
pub trait AsyncFnOnce1<Arg>: FnOnce(Arg) -> <Self as AsyncFnOnce1<Arg>>::OutputFuture {
    type OutputFuture: Future<Output = <Self as AsyncFnOnce1<Arg>>::Output>;
    type Output;
}

impl<F: ?Sized, Fut, Arg> AsyncFnOnce1<Arg> for F
where
    F: FnOnce(Arg) -> Fut,
    Fut: Future,
{
    type OutputFuture = Fut;
    type Output = Fut::Output;
}
