use std::{
    net::IpAddr,
    num::NonZeroU32,
    sync::{
        LazyLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DashMapStateStore};

use crate::{CONFIG, Error};

type Limiter<T = IpAddr> = RateLimiter<T, DashMapStateStore<T>, DefaultClock>;

static LIMITER_LOGIN: LazyLock<Limiter> = LazyLock::new(|| {
    let seconds = Duration::from_secs(CONFIG.login_ratelimit_seconds());
    let burst = NonZeroU32::new(CONFIG.login_ratelimit_max_burst()).expect("Non-zero login ratelimit burst");
    RateLimiter::keyed(Quota::with_period(seconds).expect("Non-zero login ratelimit seconds").allow_burst(burst))
});

// Keykeeper: each account's own login limit, keyed by the email as typed,
// whether or not an account has it, so a refusal says nothing about which
// emails have accounts. See DESIGN.md, "Login limits".
static LIMITER_LOGIN_ACCOUNT: LazyLock<Limiter<String>> = LazyLock::new(|| {
    let seconds = Duration::from_secs(CONFIG.login_ratelimit_account_seconds());
    let burst =
        NonZeroU32::new(CONFIG.login_ratelimit_account_max_burst()).expect("Non-zero account login ratelimit burst");
    RateLimiter::keyed(
        Quota::with_period(seconds).expect("Non-zero account login ratelimit seconds").allow_burst(burst),
    )
});
static ACCOUNT_CHECKS: AtomicU32 = AtomicU32::new(0);

// Keykeeper: pre-login, which every login starts with, gets an address's login
// allowance, counted on its own, instead of the shared unauthenticated one: it
// only says how to derive the key, the same way for an email without an
// account.
static LIMITER_PRELOGIN: LazyLock<Limiter> = LazyLock::new(|| {
    let seconds = Duration::from_secs(CONFIG.login_ratelimit_seconds());
    let burst = NonZeroU32::new(CONFIG.login_ratelimit_max_burst()).expect("Non-zero login ratelimit burst");
    RateLimiter::keyed(Quota::with_period(seconds).expect("Non-zero login ratelimit seconds").allow_burst(burst))
});

static LIMITER_ADMIN: LazyLock<Limiter> = LazyLock::new(|| {
    let seconds = Duration::from_secs(CONFIG.admin_ratelimit_seconds());
    let burst = NonZeroU32::new(CONFIG.admin_ratelimit_max_burst()).expect("Non-zero admin ratelimit burst");
    RateLimiter::keyed(Quota::with_period(seconds).expect("Non-zero admin ratelimit seconds").allow_burst(burst))
});

static LIMITER_UNAUTHENTICATED: LazyLock<Limiter> = LazyLock::new(|| {
    let seconds = Duration::from_secs(CONFIG.unauthenticated_ratelimit_seconds());
    let burst = NonZeroU32::new(CONFIG.unauthenticated_ratelimit_max_burst())
        .expect("Non-zero unauthenticated ratelimit burst");
    RateLimiter::keyed(
        Quota::with_period(seconds).expect("Non-zero unauthenticated ratelimit seconds").allow_burst(burst),
    )
});

pub fn check_limit_unauthenticated(ip: &IpAddr) -> Result<(), Error> {
    match LIMITER_UNAUTHENTICATED.check_key(ip) {
        Ok(()) => Ok(()),
        Err(_e) => {
            err_code!("Too many requests", 429);
        }
    }
}

pub fn check_limit_login(ip: &IpAddr) -> Result<(), Error> {
    match LIMITER_LOGIN.check_key(ip) {
        Ok(()) => Ok(()),
        Err(_e) => {
            err_code!("Too many login requests", 429);
        }
    }
}

pub fn check_limit_prelogin(ip: &IpAddr) -> Result<(), Error> {
    match LIMITER_PRELOGIN.check_key(ip) {
        Ok(()) => Ok(()),
        Err(_e) => {
            err_code!("Too many requests", 429);
        }
    }
}

/// The key for an email as typed: trimmed, lowercase, and no longer than an
/// email can be.
fn account_key(username: &str) -> String {
    username.trim().to_lowercase().chars().take(320).collect()
}

/// Whether a login for this email may go ahead under this limiter.
fn account_allowed(limiter: &Limiter<String>, username: &str) -> bool {
    limiter.check_key(&account_key(username)).is_ok()
}

pub fn check_limit_login_account(username: &str) -> Result<(), Error> {
    // Keys are whatever people type, so now and then forget the ones whose
    // limit has filled back up.
    if ACCOUNT_CHECKS.fetch_add(1, Ordering::Relaxed) % 1024 == 1023 {
        LIMITER_LOGIN_ACCOUNT.retain_recent();
        LIMITER_LOGIN_ACCOUNT.shrink_to_fit();
    }
    if account_allowed(&LIMITER_LOGIN_ACCOUNT, username) {
        Ok(())
    } else {
        err_code!("Too many login attempts for this account. Wait a minute and try again.", 429);
    }
}

pub fn check_limit_admin(ip: &IpAddr) -> Result<(), Error> {
    match LIMITER_ADMIN.check_key(ip) {
        Ok(()) => Ok(()),
        Err(_e) => {
            err_code!("Too many admin requests", 429);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, time::Duration};

    use governor::{Quota, RateLimiter};

    use super::{Limiter, account_allowed};

    fn limiter(burst: u32) -> Limiter<String> {
        RateLimiter::keyed(
            Quota::with_period(Duration::from_secs(3600)).unwrap().allow_burst(NonZeroU32::new(burst).unwrap()),
        )
    }

    #[test]
    fn an_account_gets_its_burst_then_waits() {
        let limiter = limiter(3);
        for _ in 0..3 {
            assert!(account_allowed(&limiter, "dana@acme.test"));
        }
        assert!(!account_allowed(&limiter, "dana@acme.test"));
        // Another account, from anywhere, is unaffected.
        assert!(account_allowed(&limiter, "lee@acme.test"));
    }

    #[test]
    fn the_same_email_typed_differently_shares_one_limit() {
        let limiter = limiter(2);
        assert!(account_allowed(&limiter, "Dana@Acme.test"));
        assert!(account_allowed(&limiter, "  dana@acme.test "));
        assert!(!account_allowed(&limiter, "DANA@ACME.TEST"));
    }

    #[test]
    fn an_email_without_an_account_is_limited_the_same_way() {
        // Keyed by what was typed, so nothing tells which emails have accounts.
        let limiter = limiter(1);
        assert!(account_allowed(&limiter, "nobody@nowhere.test"));
        assert!(!account_allowed(&limiter, "nobody@nowhere.test"));
        let long = "x".repeat(10_000);
        assert!(account_allowed(&limiter, &long));
        assert!(!account_allowed(&limiter, &format!("{long}y")));
    }
}
