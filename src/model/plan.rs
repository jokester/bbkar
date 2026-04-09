use crate::Timestamp;

pub struct RunPlan {
    pub steps: Vec<RunStep>,
}

#[derive(Debug)]
pub enum RunStep {
    SendFull(Timestamp),
    SendIncremental(Timestamp, Timestamp),
}

pub struct RestorePlan {
    pub steps: Vec<RestoreStep>,
}

#[derive(Debug)]
pub enum RestoreStep {
    ReceiveFull(Timestamp),
    ReceiveIncremental(Timestamp, Timestamp), // (snapshot, parent)
}
