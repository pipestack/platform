use pipeline_manager::config_converter::Pipeline;
use schemars::{generate::SchemaSettings};

fn main() {
    let generator = SchemaSettings::draft07().into_generator();
    let schema = generator.into_root_schema_for::<Pipeline>();
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
