use schemars::generate::SchemaSettings;
use shared::Pipeline;

fn main() {
    let generator = SchemaSettings::draft07().into_generator();
    let schema = generator.into_root_schema_for::<Pipeline>();
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
