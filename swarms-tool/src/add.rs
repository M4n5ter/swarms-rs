use swarms_macro::tool;
use thiserror::Error;

#[tool(
    name = "add",
    description = "
    Add two numbers together.
    ",
    arg(x, description = "A Complex parameter"),
    arg(y, description = "Another Complex parameter")
)]
pub fn add(x: f64, y: f64) -> Result<f64, AddError> {
    Ok(x + y)
}

#[derive(Debug, Error)]
pub enum AddError {}
