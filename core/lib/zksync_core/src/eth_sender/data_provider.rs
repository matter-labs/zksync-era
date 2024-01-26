pub trait DataProvider {}

#[derive(Debug)]
pub struct Rollup {}

#[derive(Debug)]
pub struct Validium {}

impl DataProvider for Rollup {}

impl DataProvider for Validium {}
