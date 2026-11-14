#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    Model,
    Stock,
    Origin,
    Operations,
    Operation(usize),
}
