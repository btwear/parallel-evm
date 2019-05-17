use ethcore::trace::trace::{Action, Res};
use ethcore::trace::FlatTrace;
use ethereum_types::{Address, U256};

pub fn get_related_addresses(trace: &Vec<FlatTrace>) -> Vec<Address> {
    let mut addresses = vec![];
    for sub_trace in trace {
        let (from, to) = match &sub_trace.action {
            Action::Call(call) => (call.from, call.to),
            Action::Create(create) => (
                create.from,
                match &sub_trace.result {
                    Res::Create(create_result) => create_result.address,
                    _ => Address::zero(),
                },
            ),
            Action::Suicide(suicide) => (suicide.refund_address, suicide.address),
            Action::Reward(reward) => (Address::zero(), reward.author),
        };
        if addresses.is_empty() && from != Address::zero() {
            addresses.push(from);
        }
        addresses.push(to);
    }
    addresses
}
