import os
import re
import mdformat
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

known_types = []


def fix_links(text: str) -> str:
    text = re.sub(r'(?<![(<"\[])(https?://[^\s<>)"]+)', r"<\1>", text)

    return text


def backtick_types(text: str) -> str:
    if not known_types:
        return text

    sorted_types = sorted(known_types, key=len, reverse=True)
    type_pat = re.compile(
        r"(?<![`\[/\w])("
        + "|".join(re.escape(t) + r"s?" for t in sorted_types)
        + r")(?![`\]\w])"
    )

    protected = re.compile(r"(`[^`]+`|\[[^\]]*\]\([^)]*\)|<https?://[^>]+>)")
    parts = protected.split(text)

    result = []
    for i, part in enumerate(parts):
        if i % 2 == 1:
            result.append(part)
        else:
            result.append(type_pat.sub(r"`\1`", part))

    return "".join(result)


def fix_missing_backticks(text: str) -> str:
    # Link labels missing backticks for known types
    def fix_link_label(m: re.Match) -> str:
        label = m.group(1)
        href = m.group(2)
        clean = label.strip("`")
        if clean in known_types and not label.startswith("`"):
            return f"[`{clean}`]({href})"
        return m.group(0)

    text = re.sub(r"\[([^\]]+)\]\(([^)]*)\)", fix_link_label, text)

    # identifiers with underscores bolded but not backticked
    text = re.sub(
        r"\*\*(?!`)([a-z][a-z0-9]*(?:_[a-z0-9]+)+)(?<!`)\*\*", r"**`\1`**", text
    )
    # quoted identifiers with underscores
    text = re.sub(r'"([a-z][a-z0-9]*(?:_[a-z0-9]+)+)"', r'"`\1`"', text)

    return text


def normalize_md(text: str) -> str:
    try:
        return mdformat.text(text)
    except Exception:
        return text


def process_item(rec: dict[str, Any], parent: Optional[str]):
    name = schema.avro_field_name(rec["name"])
    doc = rec.get("doc", "")

    if parent:
        name = f"{parent}_{name}"

    if doc and name:
        with open(f"docs/gen/{name}.md", "w") as f:
            if isinstance(doc, list):
                content = "\n".join(doc)
            else:
                content = doc
            content = fix_links(content)
            content = fix_missing_backticks(content)
            content = backtick_types(content)
            content = normalize_md(content)

            f.write(content)

    if rec["type"] == "record":
        for field in rec.get("fields", []):
            process_item(field, name)


# collect pass
for rec in j:
    if (
        rec["type"] in ["record", "enum", "map", "union"]
        and "abstract" not in rec.keys()
    ):
        name = schema.avro_field_name(rec["name"])
        known_types.append(name)

# process pass
for rec in j:
    if (
        rec["type"] in ["record", "enum", "map", "union"]
        and "abstract" not in rec.keys()
    ):
        process_item(rec, None)
