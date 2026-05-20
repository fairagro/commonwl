import os
from typing import Any, Final, Optional, cast
from schema_salad.ref_resolver import Loader, file_uri
import schema_salad.schema as schema
import schema_salad.jsonld_context as jsonld_context
from ruamel.yaml.comments import CommentedSeq

schema_uri = "testdata/cwl/CommonWorkflowLanguage.yml"
_, _, metaschema_loader = schema.get_metaschema()
schema_uri = file_uri(os.path.abspath(schema_uri))
schema_raw_doc: Final = metaschema_loader.fetch(schema_uri)
schema_doc, schema_metadata = metaschema_loader.resolve_all(schema_raw_doc, schema_uri)

metactx: Final = schema.collect_namespaces(schema_metadata)
if "$base" in schema_metadata:
    metactx["@base"] = schema_metadata["$base"]
if isinstance(schema_doc, CommentedSeq):
    schema_ctx, rdfs = jsonld_context.salad_to_jsonld_context(schema_doc, metactx)
else:
    raise Exception("damn")

schema_version: Final = schema_metadata.get("saladVersion", None)
document_loader: Final = Loader(
    schema_ctx, skip_schemas=False, salad_version=schema_version
)

i = cast(list[dict[str, Any]], schema_doc)
j = schema.extend_and_specialize(i, document_loader)

os.makedirs("docs/gen", exist_ok=True)


def process_item(rec: dict[str, Any], parent: Optional[str]):
    name = schema.avro_field_name(rec["name"])
    doc = rec.get("doc", "")

    if parent:
        name = f"{parent}_{name}"

    if doc and name:
        with open(f"docs/gen/{name}.md", "w") as f:
            if isinstance(doc, list):
                f.write("\n".join(doc))
            else:
                f.write(doc)

    if rec["type"] == "record":
        for field in rec.get("fields", []):
            process_item(field, name)


for rec in j:
    if (
        rec["type"] in ["record", "enum", "map", "union"]
        and "abstract" not in rec.keys()
    ):
        process_item(rec, None)
