/// Data-only ordered operations; no machining logic in this version.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Placeholder,
}
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    NotCalculated,
    Ready,
    Error,
}
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub tool_id: Option<u64>,
    pub toolpath_id: Option<u64>,
}
#[derive(Default)]
pub struct Operations {
    pub items: Vec<Operation>,
}
