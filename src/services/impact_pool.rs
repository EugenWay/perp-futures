use crate::types::Timestamp;

pub trait ImpactPoolService {
    fn distribute(&self, _now: Timestamp) {
        // TODO
    }
}

#[derive(Default)]
pub struct BasicImpactPoolService;

impl ImpactPoolService for BasicImpactPoolService {}
