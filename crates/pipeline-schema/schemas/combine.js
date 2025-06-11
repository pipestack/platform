import $RefParser from "@apidevtools/json-schema-ref-parser";

try {
  let clonedSchema = await $RefParser.dereference("./main.schema.json", { mutateInputSchema: false });
  console.log(JSON.stringify(clonedSchema, undefined, 2));
} catch (err) {
  console.error(err);
}