use lom::{
    config::{parse_config, parse_theme},
    model::Model,
    runtime::parse_fixture,
};

pub const CONFIG: &str = include_str!("../../examples/minimal/config.kdl");
pub const THEME: &str = include_str!("../../examples/minimal/theme.kdl");
pub const FIXTURE: &str = include_str!("../../examples/minimal/fixture.kdl");

pub fn model() -> Model {
    parse_fixture(
        FIXTURE,
        parse_config(CONFIG).unwrap(),
        parse_theme(THEME).unwrap(),
    )
    .unwrap()
}
