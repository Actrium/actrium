use std::cell::Cell;

use actr_protocol::uri::ActrUriError;
use actrium_core::realm_link::{can_share_realm_link, open_realm_link};

const INSTALLATION_ID: &str = "abcdefghijklmnopqrstuvwxyz";
const FOREIGN_INSTALLATION_ID: &str = "bcdefghijklmnopqrstuvwxyza";

#[test]
fn matching_local_realm_uses_local_record_without_network_discovery() {
    let local_record_reads = Cell::new(0);
    let network_requests = Cell::new(0);
    let input = format!("actr://{INSTALLATION_ID}.actr.alt/h3x8p2wq");

    let opened = open_realm_link(
        &input,
        INSTALLATION_ID,
        |uri| {
            local_record_reads.set(local_record_reads.get() + 1);
            assert_eq!(
                uri.realm_name().as_deref(),
                Some("h3x8p2wq@abcdefghijklmnopqrstuvwxyz.actr.alt")
            );
            Ok("local-record")
        },
        |_| -> Result<&str, ActrUriError> {
            network_requests.set(network_requests.get() + 1);
            panic!("local Realm entered network discovery")
        },
    )
    .unwrap();

    assert_eq!(opened, "local-record");
    assert_eq!(local_record_reads.get(), 1);
    assert_eq!(network_requests.get(), 0);
}

#[test]
fn foreign_local_realm_fails_before_network_or_state_mutation() {
    let local_record_reads = Cell::new(0);
    let network_requests = Cell::new(0);
    let state_mutations = Cell::new(0);
    let input = format!("actr://{FOREIGN_INSTALLATION_ID}.actr.alt/h3x8p2wq");

    let error = open_realm_link(
        &input,
        INSTALLATION_ID,
        |_| -> Result<&str, ActrUriError> {
            local_record_reads.set(local_record_reads.get() + 1);
            state_mutations.set(state_mutations.get() + 1);
            panic!("foreign local Realm accessed a local record")
        },
        |_| -> Result<&str, ActrUriError> {
            network_requests.set(network_requests.get() + 1);
            state_mutations.set(state_mutations.get() + 1);
            panic!("foreign local Realm entered network discovery")
        },
    )
    .unwrap_err();

    assert_eq!(error.code(), "URI_LOCAL_REALM_FOREIGN");
    assert_eq!(local_record_reads.get(), 0);
    assert_eq!(network_requests.get(), 0);
    assert_eq!(state_mutations.get(), 0);
}

#[test]
fn dns_realm_uses_network_discovery_without_installation_comparison() {
    let local_record_reads = Cell::new(0);
    let network_requests = Cell::new(0);

    let opened = open_realm_link(
        "actr://acme.com/k7m2q9xh",
        INSTALLATION_ID,
        |_| -> Result<&str, ActrUriError> {
            local_record_reads.set(local_record_reads.get() + 1);
            panic!("DNS Realm accessed a local record")
        },
        |uri| {
            network_requests.set(network_requests.get() + 1);
            assert_eq!(uri.domain(), "acme.com");
            Ok("dns-discovery")
        },
    )
    .unwrap();

    assert_eq!(opened, "dns-discovery");
    assert_eq!(local_record_reads.get(), 0);
    assert_eq!(network_requests.get(), 1);
}

#[test]
fn authority_only_links_fail_before_either_resolver_is_entered() {
    for input in [
        "actr://acme.com".to_string(),
        format!("actr://{INSTALLATION_ID}.actr.alt"),
    ] {
        let local_record_reads = Cell::new(0);
        let network_requests = Cell::new(0);

        let error = open_realm_link(
            &input,
            INSTALLATION_ID,
            |_| -> Result<(), ActrUriError> {
                local_record_reads.set(local_record_reads.get() + 1);
                Ok(())
            },
            |_| -> Result<(), ActrUriError> {
                network_requests.set(network_requests.get() + 1);
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(error.code(), "URI_LABEL_INVALID", "{input}");
        assert_eq!(local_record_reads.get(), 0, "{input}");
        assert_eq!(network_requests.get(), 0, "{input}");
    }
}

#[test]
fn sharing_is_disabled_for_every_local_realm_before_opening() {
    let owned = format!("actr://{INSTALLATION_ID}.actr.alt/h3x8p2wq");
    let foreign = format!("actr://{FOREIGN_INSTALLATION_ID}.actr.alt/h3x8p2wq");

    assert!(!can_share_realm_link(&owned));
    assert!(!can_share_realm_link(&foreign));
    assert!(can_share_realm_link("actr://acme.com/k7m2q9xh"));
    assert!(!can_share_realm_link("actr://acme.com"));
    assert!(!can_share_realm_link("not a Realm link"));
}
