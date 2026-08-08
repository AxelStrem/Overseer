//! Who may ask this server for a document.
//!
//! One shared secret, checked the same way for a browser and for an agent. The tests that
//! matter are the refusals, and the one about starting up: a server reachable from beyond
//! this machine without a token would publish a personal record to whoever finds the port.

use overseer::server::{refuse_to_start, Access};
use std::net::IpAddr;

const TOKEN: &str = "correct-horse-battery-staple";

fn loopback() -> IpAddr {
    "127.0.0.1".parse().unwrap()
}
fn everywhere() -> IpAddr {
    "0.0.0.0".parse().unwrap()
}

#[test]
fn without_a_token_anyone_may_ask() {
    let open = Access::unrestricted();
    assert!(open.permits(None, None));
    assert!(!open.is_restricted());
}

#[test]
fn with_a_token_the_token_is_required() {
    let access = Access::with_token(TOKEN);
    assert!(!access.permits(None, None), "an unauthenticated request was allowed");
    assert!(access.permits(Some(&format!("Bearer {}", TOKEN)), None));
    // A browser cannot set a header on a pasted link, so the cookie counts too.
    assert!(access.permits(None, Some(TOKEN)));
}

#[test]
fn a_wrong_token_is_refused_however_it_arrives() {
    let access = Access::with_token(TOKEN);
    for wrong in ["", "wrong", "correct-horse-battery-stapl", "correct-horse-battery-staplee"] {
        assert!(
            !access.permits(Some(&format!("Bearer {}", wrong)), None),
            "'{}' was accepted as the token",
            wrong
        );
        assert!(!access.permits(None, Some(wrong)), "'{}' was accepted as a cookie", wrong);
    }
}

#[test]
fn a_token_without_the_scheme_is_not_a_bearer_token() {
    let access = Access::with_token(TOKEN);
    assert!(
        !access.permits(Some(TOKEN), None),
        "a bare Authorization value was treated as a bearer token"
    );
}

#[test]
fn it_will_not_listen_beyond_this_machine_without_a_token() {
    assert!(
        refuse_to_start(&everywhere(), &Access::unrestricted()).is_some(),
        "an open server was allowed to listen on every interface"
    );
    assert!(
        refuse_to_start(&everywhere(), &Access::with_token(TOKEN)).is_none(),
        "a server with a token was refused"
    );
    assert!(
        refuse_to_start(&loopback(), &Access::unrestricted()).is_none(),
        "a loopback server was refused, which would make the local workflow need a token"
    );
}
