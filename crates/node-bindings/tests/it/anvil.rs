use alloy_node_bindings::Anvil;

#[test]
#[ignore]
fn can_launch_anvil() {
    if !ci_info::is_ci() {
        return;
    }

    let _ = Anvil::new().spawn();
}

#[test]
#[ignore]
fn can_launch_anvil_with_more_accounts() {
    if !ci_info::is_ci() {
        return;
    }

    let _ = Anvil::new().arg("--accounts").arg("20").spawn();
}

#[test]
#[ignore]
fn assert_chain_id() {
    if !ci_info::is_ci() {
        return;
    }

    let id = 99999;
    let anvil = Anvil::new().chain_id(id).spawn();
    assert_eq!(anvil.chain_id(), id);
}

#[test]
#[ignore]
fn assert_chain_id_without_rpc() {
    if !ci_info::is_ci() {
        return;
    }

    let anvil = Anvil::new().spawn();
    assert_eq!(anvil.chain_id(), 31337);
}
