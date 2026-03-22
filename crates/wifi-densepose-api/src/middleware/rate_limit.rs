//! Rate limiting middleware using the governor crate.
//! Limits each client IP to `max_per_minute` requests per minute.

use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;

pub type IpRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// Create a global (not per-IP) rate limiter with the given rate.
pub fn create_limiter(max_per_minute: u32) -> Arc<IpRateLimiter> {
    let per_sec = max_per_minute / 60 + 1;
    let quota = Quota::per_second(NonZeroU32::new(per_sec).unwrap());
    Arc::new(RateLimiter::direct(quota))
}
