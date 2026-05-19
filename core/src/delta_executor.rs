use crate::delta::{AnyDelta, Delta};
use crate::error::Result;

#[allow(async_fn_in_trait)]
pub trait DeltaExecutor<M> {
    async fn apply_delta(&mut self, delta: Delta<M>) -> Result<()>;
}

#[allow(async_fn_in_trait)]
pub trait AnyDeltaExecutor {
    async fn apply_any_delta(&mut self, delta: AnyDelta) -> Result<()>;
}
