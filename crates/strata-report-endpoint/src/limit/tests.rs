//! The tests of the limiter and of the network of a sender.

use super::*;

const HOUR: Duration = Duration::from_secs(3600);

#[test]
fn a_sender_spends_its_limit_and_then_waits_until_the_oldest_request_expires() {
    let mut limiter = Limiter::new(2, HOUR);
    let start = Instant::now();
    assert!(limiter.allow("a", 1, start).is_ok());
    assert!(
        limiter
            .allow("a", 1, start + Duration::from_secs(60))
            .is_ok()
    );
    let wait = limiter
        .allow("a", 1, start + Duration::from_secs(120))
        .unwrap_err();
    assert_eq!(wait, Denied::Wait(HOUR - Duration::from_secs(120)));
    assert!(
        limiter.allow("a", 1, start + HOUR).is_ok(),
        "the first request expired"
    );
}

#[test]
fn senders_do_not_share_a_limit_and_a_refused_request_spends_nothing() {
    let mut limiter = Limiter::new(1, HOUR);
    let now = Instant::now();
    assert!(limiter.allow("a", 1, now).is_ok());
    assert!(limiter.allow("b", 1, now).is_ok());
    assert!(limiter.allow("a", 1, now).is_err());
    assert!(limiter.allow("a", 1, now + HOUR).is_ok());
}

#[test]
fn a_cost_above_the_limit_is_over_and_never_a_wait_and_a_cost_spends_that_many_requests() {
    let mut limiter = Limiter::new(3, HOUR);
    let now = Instant::now();
    assert_eq!(limiter.allow("all", 4, now), Err(Denied::Over));
    assert_eq!(
        limiter.allow("all", 4, now + 2 * HOUR),
        Err(Denied::Over),
        "a wait does not help a cost above the limit"
    );
    assert!(limiter.allow("all", 2, now).is_ok());
    assert!(
        matches!(limiter.allow("all", 2, now), Err(Denied::Wait(_))),
        "only one request is left"
    );
    assert!(limiter.allow("all", 1, now).is_ok());
}

#[test]
fn the_room_is_what_the_sender_can_still_spend_and_a_spend_takes_it() {
    let mut limiter = Limiter::new(5, HOUR);
    let now = Instant::now();
    assert_eq!(limiter.room("a", now), 5);
    limiter.spend("a", 3, now);
    assert_eq!(limiter.room("a", now), 2);
    assert_eq!(limiter.room("b", now), 5);
    assert_eq!(
        limiter.wait("a", 3, now),
        HOUR,
        "3 of the 5 are spent, so 3 more wait"
    );
    assert_eq!(limiter.wait("a", 2, now), Duration::ZERO);
    limiter.spend("a", 0, now);
    assert_eq!(limiter.room("a", now), 2, "a spend of 0 changes nothing");
    assert_eq!(limiter.room("a", now + HOUR), 5, "the spends expired");
}

#[test]
fn a_new_sender_of_a_full_table_takes_the_place_of_the_oldest_sender_and_is_never_refused() {
    let mut limiter = Limiter::new(1, HOUR);
    let start = Instant::now();
    limiter.allow("first", 1, start).unwrap();
    for n in 1..MOST_SENDERS {
        // one sender spends later than the others, so it is never the oldest
        let at = if n == 5 { 900 } else { 600 };
        limiter
            .allow(&format!("sender-{n}"), 1, start + Duration::from_secs(at))
            .unwrap();
    }
    assert_eq!(limiter.senders(), MOST_SENDERS);
    assert!(
        limiter
            .allow("one-more", 1, start + Duration::from_secs(700))
            .is_ok(),
        "the new sender is taken"
    );
    assert_eq!(limiter.senders(), MOST_SENDERS, "the table did not grow");
    assert!(
        limiter
            .allow("first", 1, start + Duration::from_secs(701))
            .is_ok(),
        "the oldest sender was dropped, so it is new again"
    );
    assert!(
        limiter
            .allow("sender-5", 1, start + Duration::from_secs(702))
            .is_err(),
        "a sender that spent later is kept"
    );
}

#[test]
fn the_limiter_holds_no_address_and_a_new_limiter_hashes_it_in_another_way() {
    let mut one = Limiter::new(1, HOUR);
    let mut two = Limiter::new(1, HOUR);
    let now = Instant::now();
    one.allow("203.0.113.7", 1, now).unwrap();
    two.allow("203.0.113.7", 1, now).unwrap();
    let held = |l: &Limiter| *l.seen.keys().next().unwrap();
    assert_ne!(held(&one), held(&two), "the salt differs for each process");
    let shown = format!("{one:?}");
    assert!(!shown.contains("203.0.113.7"), "{shown}");
}

#[test]
fn a_flood_of_senders_drops_the_old_ones_and_the_expired_senders_are_gone_at_the_hour() {
    let mut limiter = Limiter::new(1, HOUR);
    let now = Instant::now();
    for n in 0..MOST_SENDERS {
        limiter.allow(&format!("sender-{n}"), 1, now).unwrap();
    }
    assert_eq!(limiter.senders(), MOST_SENDERS);
    assert!(limiter.allow("one-more", 1, now).is_ok());
    assert_eq!(limiter.senders(), MOST_SENDERS);
    assert!(limiter.allow("two-more", 1, now + HOUR).is_ok());
    assert_eq!(limiter.senders(), 1, "the expired senders are gone");
}

#[test]
fn an_ipv6_address_counts_by_its_64_bit_network_and_other_text_stays_as_it_is() {
    assert_eq!(
        network_of("2001:db8:1:2:aaaa:bbbb:cccc:dddd"),
        network_of("2001:db8:1:2::1")
    );
    assert_eq!(network_of("2001:db8:1:2::1"), "2001:db8:1:2::/64");
    assert_ne!(
        network_of("2001:db8:1:2::1"),
        network_of("2001:db8:1:3::1"),
        "another /64 is another sender"
    );
    assert_eq!(network_of("203.0.113.7"), "203.0.113.7");
    assert_eq!(network_of("::ffff:203.0.113.7"), "203.0.113.7");
    assert_eq!(network_of("not an address"), "not an address");
    assert_eq!(network_of(""), "");
}

#[test]
fn one_ipv6_network_is_one_sender_whatever_the_host_part_of_the_address_is() {
    let mut limiter = Limiter::new(10, HOUR);
    let now = Instant::now();
    for n in 0..10 {
        let address = network_of(&format!("2001:db8:1:2::{n:x}"));
        assert!(limiter.allow(&address, 1, now).is_ok());
    }
    assert_eq!(limiter.senders(), 1, "10 hosts of one /64 are one sender");
    assert!(
        limiter
            .allow(&network_of("2001:db8:1:2:ffff::1"), 1, now)
            .is_err()
    );
}

#[test]
fn a_sender_stays_in_the_table_until_its_span_ends() {
    let mut limiter = Limiter::new(1, HOUR);
    let start = Instant::now();
    limiter.allow("a", 1, start).unwrap();
    limiter
        .allow("b", 1, start + HOUR - Duration::from_secs(1))
        .unwrap();
    limiter.forget_expired(start + HOUR - Duration::from_secs(1));
    assert_eq!(limiter.senders(), 2, "the hash of a lives for the hour");
    limiter.forget_expired(start + HOUR);
    assert_eq!(limiter.senders(), 1, "a is gone, and b stays");
}
